/* Copyright (C) 2026 smokeyxd

This program is free software; you can redistribute it and/or modify it
under the terms of the GNU General Public License as published by the
Free Software Foundation; either version 3, or (at your option) any
later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program; see the file LICENSE. If not, see
<http://www.gnu.org/licenses/>.  */

#include <arpa/inet.h>
#include <ctype.h>
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#ifndef RPC_PORT
#define RPC_PORT 8000
#endif
#define DISCOVERY_PORT 1010
#define DISCOVERY_MAGIC 0xFFFFAAAAu
#define MAX_CLIENTS 4
#define CHECK_MS 2000
#define RESEND_MS 10000
#define REOPEN_MS 5000
#define TITLE_MAX 32

typedef struct notify_request {
  char useless1[45];
  char message[3075];
} notify_request_t;

int sceKernelSendNotificationRequest(int, notify_request_t *, size_t, int);
int sceSystemServiceGetAppIdOfRunningBigApp(void);
int sceSystemServiceGetAppTitleId(int app_id, char *title_id);


static void
notify(const char *msg) {
  notify_request_t req;

  bzero(&req, sizeof req);
  snprintf(req.message, sizeof req.message, "%s", msg);
  sceKernelSendNotificationRequest(0, &req, sizeof req, 0);
}


// the wall clock can jump when the console syncs its time; intervals use this instead
static int64_t
now_ms(void) {
  struct timespec ts;

  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (int64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}


static int
open_socket(int type, int port) {
  struct sockaddr_in addr;
  int one = 1;
  int fd;

  if((fd = socket(AF_INET, type, 0)) < 0) {
    return -1;
  }

  // No SO_REUSEPORT on purpose: etaHEN's RPC server (and ps5debug on 1010) set it, and
  // leaving it off makes our bind fail while they hold the port, so we step aside.
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);

  bzero(&addr, sizeof addr);
  addr.sin_family = AF_INET;
  addr.sin_addr.s_addr = htonl(INADDR_ANY);
  addr.sin_port = htons(port);
  if(bind(fd, (struct sockaddr *)&addr, sizeof addr) < 0 ||
     (type == SOCK_STREAM && listen(fd, MAX_CLIENTS) < 0)) {
    int err = errno;
    close(fd);
    errno = err;
    return -1;
  }

  return fd;
}


// Same lines as etaHEN's Discord RPC server, so the PC app can't tell them apart.
static void
current_title(char *line, size_t size) {
  char tid[256];
  int app_id = sceSystemServiceGetAppIdOfRunningBigApp();
  size_t i;

  if(app_id < 0) {
    snprintf(line, size, "No game running.\n");
    return;
  }

  bzero(tid, sizeof tid);
  if(sceSystemServiceGetAppTitleId(app_id, tid) != 0) {
    snprintf(line, size, "Failed to get title ID.\n");
    return;
  }

  for(i = 0; i < sizeof tid && tid[i]; i++) {
    if(i >= TITLE_MAX || !(isalnum((unsigned char)tid[i]) || tid[i] == '_' || tid[i] == '-')) {
      snprintf(line, size, "Failed to get title ID.\n");
      return;
    }
  }
  if(i == 0) {
    snprintf(line, size, "Failed to get title ID.\n");
    return;
  }

  snprintf(line, size, "%s\n", tid);
}


static void
answer_discovery(int fd) {
  struct sockaddr_in from;
  socklen_t fromlen = sizeof from;
  uint32_t magic = 0;

  if(recvfrom(fd, &magic, sizeof magic, MSG_DONTWAIT, (struct sockaddr *)&from,
              &fromlen) == sizeof magic && magic == DISCOVERY_MAGIC) {
    sendto(fd, &magic, sizeof magic, MSG_DONTWAIT, (struct sockaddr *)&from, fromlen);
  }
}


static void
drop_client(struct pollfd *fds, int i, int *nclients) {
  close(fds[i].fd);
  fds[i] = fds[2 + *nclients - 1];
  (*nclients)--;
}


int
main(void) {
  struct pollfd fds[2 + MAX_CLIENTS];
  char msg[128];
  char line[64] = "";
  char now[64];
  int64_t checked_at = 0;
  int64_t sent_at = 0;
  int64_t server_down_at = 0;
  int64_t udp_down_at = 0;
  int nclients = 0;
  int server;
  int udp;
  int force;
  int i;

  signal(SIGPIPE, SIG_IGN);

  if((server = open_socket(SOCK_STREAM, RPC_PORT)) < 0) {
    if(errno == EADDRINUSE) {
      snprintf(msg, sizeof msg, "ps5-rpc: port %d is already in use (etaHEN's RPC or ps5-rpc is already running)", RPC_PORT);
    } else {
      snprintf(msg, sizeof msg, "ps5-rpc: could not open port %d", RPC_PORT);
    }
    notify(msg);
    return 0;
  }
  if((udp = open_socket(SOCK_DGRAM, DISCOVERY_PORT)) < 0) {
    udp_down_at = now_ms();
  }

  snprintf(msg, sizeof msg, "ps5-rpc: ready on port %d", RPC_PORT);
  notify(msg);

  for(;;) {
    fds[0].fd = server;
    fds[0].events = POLLIN;
    fds[0].revents = 0;
    fds[1].fd = udp;
    fds[1].events = POLLIN;
    fds[1].revents = 0;

    if(poll(fds, 2 + nclients, CHECK_MS) < 0) {
      if(errno != EINTR) {
        usleep(500000);
      }
      continue;
    }
    force = 0;

    // A dead socket reports an error on every poll; reopening beats spinning at 100% CPU
    // after the network goes away (rest mode, cable pulled, Wi-Fi drop).
    if(server >= 0 && (fds[0].revents & (POLLERR | POLLHUP | POLLNVAL))) {
      close(server);
      server = -1;
      server_down_at = now_ms();
    } else if(server >= 0 && (fds[0].revents & POLLIN)) {
      int fd = accept(server, 0, 0);
      if(fd >= 0) {
        // newest wins, so nobody can lock the PC app out by holding every slot
        if(nclients == MAX_CLIENTS) {
          drop_client(fds, 2, &nclients);
        }
        fds[2 + nclients].fd = fd;
        fds[2 + nclients].events = POLLIN;
        fds[2 + nclients].revents = 0;
        nclients++;
        force = 1;
      } else if(errno != EINTR && errno != EAGAIN && errno != ECONNABORTED) {
        usleep(500000);
      }
    }
    if(server < 0 && now_ms() - server_down_at >= REOPEN_MS) {
      if((server = open_socket(SOCK_STREAM, RPC_PORT)) < 0) {
        server_down_at = now_ms();
      }
    }

    if(udp >= 0 && (fds[1].revents & (POLLERR | POLLHUP | POLLNVAL))) {
      close(udp);
      udp = -1;
      udp_down_at = now_ms();
    } else if(udp >= 0 && (fds[1].revents & POLLIN)) {
      answer_discovery(udp);
    }
    if(udp < 0 && now_ms() - udp_down_at >= 6 * REOPEN_MS) {
      if((udp = open_socket(SOCK_DGRAM, DISCOVERY_PORT)) < 0) {
        udp_down_at = now_ms();
      }
    }

    // read-only server: anything a client sends is thrown away
    for(i = 2; i < 2 + nclients;) {
      char junk[256];
      ssize_t n = 1;
      if(fds[i].revents & (POLLIN | POLLHUP | POLLERR | POLLNVAL)) {
        n = recv(fds[i].fd, junk, sizeof junk, MSG_DONTWAIT);
      }
      if(n == 0 || (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK && errno != EINTR)) {
        drop_client(fds, i, &nclients);
        continue;
      }
      i++;
    }

    if(nclients == 0 || (!force && now_ms() - checked_at < CHECK_MS)) {
      continue;
    }
    checked_at = now_ms();
    current_title(now, sizeof now);
    if(!force && strcmp(now, line) == 0 && now_ms() - sent_at < RESEND_MS) {
      continue;
    }
    snprintf(line, sizeof line, "%s", now);
    sent_at = now_ms();

    // MSG_DONTWAIT: a client that stops reading gets dropped instead of
    // blocking the loop once its socket buffer fills up
    for(i = 2; i < 2 + nclients;) {
      if(send(fds[i].fd, line, strlen(line), MSG_NOSIGNAL | MSG_DONTWAIT) <= 0) {
        drop_client(fds, i, &nclients);
        continue;
      }
      i++;
    }
  }

  return 0;
}
