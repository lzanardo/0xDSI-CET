#include "cet.h"
#include "internal.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <errno.h>
#include <stdint.h>

#if defined(CET_ENABLE_MMAP_ARENA)
#include <sys/mman.h>
#include <unistd.h>
#ifndef MAP_ANONYMOUS
#ifdef MAP_ANON
#define MAP_ANONYMOUS MAP_ANON
#endif
#endif
#endif

void cet_runtime_stats_set_error(cet_runtime_stats_t* stats, const char* msg) {
  if (!stats || !msg) return;
  if (stats->error[0] == '\0') {
    snprintf(stats->error, sizeof(stats->error), "%s", msg);
  }
}

void cet_runtime_stats_init(cet_runtime_stats_t* stats) {
  if (stats) memset(stats, 0, sizeof(*stats));
}

static size_t parse_size_env(const char* name, size_t fallback) {
  const char* v = getenv(name);
  if (!v || !*v) return fallback;
  char* end = NULL;
  unsigned long long parsed = strtoull(v, &end, 10);
  if (end == v) return fallback;
  return (size_t)parsed;
}

static int parse_bool_env(const char* name, int fallback) {
  const char* v = getenv(name);
  if (!v || !*v) return fallback;
  if (strcmp(v, "1") == 0 || strcmp(v, "true") == 0 || strcmp(v, "TRUE") == 0 || strcmp(v, "yes") == 0) return 1;
  if (strcmp(v, "0") == 0 || strcmp(v, "false") == 0 || strcmp(v, "FALSE") == 0 || strcmp(v, "no") == 0) return 0;
  return fallback;
}

void cet_runtime_config_default(cet_runtime_config_t* cfg) {
  if (!cfg) return;
  memset(cfg, 0, sizeof(*cfg));
  cfg->version = CET_RUNTIME_CONFIG_VERSION;
  cfg->native_threads = parse_size_env("CET_NATIVE_THREADS", 1);
  if (cfg->native_threads == 0) cfg->native_threads = 1;
  if (cfg->native_threads > CET_MAX_NATIVE_THREADS) cfg->native_threads = CET_MAX_NATIVE_THREADS;
  cfg->enable_mmap_arena = parse_bool_env("CET_ENABLE_MMAP_ARENA", 0);
  cfg->mmap_workspace_bytes = parse_size_env("CET_MMAP_WORKSPACE_BYTES", 0);
  cfg->madvise_hugepage = parse_bool_env("CET_MADVISE_HUGEPAGE", 0);
  cfg->deterministic_merge = 1;
}

static size_t align_up(size_t n, size_t a) {
  if (a == 0) return n;
  size_t r = n % a;
  return r ? (n + (a - r)) : n;
}

int cet_workspace_open(
  cet_runtime_workspace_t* ws,
  size_t bytes,
  int use_mmap,
  int madvise_hugepage,
  cet_runtime_stats_t* runtime_stats
) {
  if (!ws) return -1;
  memset(ws, 0, sizeof(*ws));
  if (bytes == 0) return 0;

  size_t cap = align_up(bytes, 4096);

#if defined(CET_ENABLE_MMAP_ARENA) && defined(MAP_ANONYMOUS)
  if (use_mmap) {
    void* p = mmap(NULL, cap, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p != MAP_FAILED) {
      ws->base = p;
      ws->capacity = cap;
      ws->offset = 0;
      ws->used_mmap = 1;
      if (runtime_stats) {
        runtime_stats->workspace_bytes = cap;
        runtime_stats->used_mmap_arena = 1;
      }
#ifdef MADV_HUGEPAGE
      if (madvise_hugepage) {
        (void)madvise(p, cap, MADV_HUGEPAGE);
      }
#else
      (void)madvise_hugepage;
#endif
      return 0;
    }
    cet_runtime_stats_set_error(runtime_stats, "mmap arena allocation failed; falling back to calloc workspace");
  }
#else
  (void)use_mmap;
  (void)madvise_hugepage;
#endif

  void* p = calloc(1, cap);
  if (!p) {
    cet_runtime_stats_set_error(runtime_stats, "workspace allocation failed");
    return -1;
  }
  ws->base = p;
  ws->capacity = cap;
  ws->offset = 0;
  ws->used_mmap = 0;
  if (runtime_stats) {
    runtime_stats->workspace_bytes = cap;
    runtime_stats->used_mmap_arena = 0;
  }
  return 0;
}

void* cet_workspace_alloc(cet_runtime_workspace_t* ws, size_t bytes, size_t align) {
  if (!ws || !ws->base || bytes == 0) return NULL;
  if (align < sizeof(void*)) align = sizeof(void*);
  size_t off = align_up(ws->offset, align);
  if (off > ws->capacity || bytes > ws->capacity - off) return NULL;
  void* p = (void*)((unsigned char*)ws->base + off);
  ws->offset = off + bytes;
  memset(p, 0, bytes);
  return p;
}

void cet_workspace_close(cet_runtime_workspace_t* ws) {
  if (!ws || !ws->base) return;
#if defined(CET_ENABLE_MMAP_ARENA)
  if (ws->used_mmap) {
    (void)munmap(ws->base, ws->capacity);
  } else
#endif
  {
    free(ws->base);
  }
  memset(ws, 0, sizeof(*ws));
}
