/*
 * cet_ffi.h — C-ABI header for the Rust CET engine.
 *
 * All handle types are opaque. Every _new function returns a non-null handle
 * on success or NULL on failure; every handle must be freed exactly once via
 * the matching _free function. Passing NULL to a _free is a no-op.
 *
 * Numeric error codes: 0 = ok, negative values indicate errors (see the Rust
 * doc comments in crates/cet-ffi/src/lib.rs for specifics).
 */

#ifndef CET_FFI_H
#define CET_FFI_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CetGraph CetGraph;
typedef struct CetQuery CetQuery;
typedef struct CetResult CetResult;
typedef struct CetStats CetStats;
typedef struct CetRuntimeConfig CetRuntimeConfig;
typedef struct CetRuntimeStats CetRuntimeStats;

/* Graph */
CetGraph* cet_ffi_graph_new(size_t max_vertices, size_t max_edges);
void      cet_ffi_graph_free(CetGraph* g);
int       cet_ffi_graph_add_vertex(CetGraph* g, int64_t id,
                                   const char* partition_key,
                                   const char* event_type,
                                   int64_t event_time_ms);
int       cet_ffi_graph_add_edge(CetGraph* g, int64_t src, int64_t dst,
                                 int64_t window_start_ms,
                                 int64_t window_end_ms);
size_t    cet_ffi_graph_vertex_count(const CetGraph* g);
size_t    cet_ffi_graph_edge_count(const CetGraph* g);

/* Query */
CetQuery* cet_ffi_query_parse(const char* name, const char* pattern_csv,
                              int64_t within_ms, int64_t slide_ms);
CetQuery* cet_ffi_query_new(const char* name);
int       cet_ffi_query_push_step(CetQuery* q, const char* event_type,
                                  int kleene_plus);
void      cet_ffi_query_set_within_ms(CetQuery* q, int64_t within_ms);
void      cet_ffi_query_set_skip_till_any_match(CetQuery* q, int flag);
void      cet_ffi_query_free(CetQuery* q);

/* Result */
CetResult* cet_ffi_result_new(size_t max_paths, size_t max_path_len);
void       cet_ffi_result_free(CetResult* r);
size_t     cet_ffi_result_path_count(const CetResult* r);
size_t     cet_ffi_result_path_len(const CetResult* r, size_t idx);
size_t     cet_ffi_result_copy_path(const CetResult* r, size_t path_idx,
                                    int64_t* out, size_t out_len);

/* Stats */
CetStats* cet_ffi_stats_new(void);
void      cet_ffi_stats_free(CetStats* s);
size_t    cet_ffi_stats_paths_emitted(const CetStats* s);
size_t    cet_ffi_stats_paths_truncated(const CetStats* s);
int       cet_ffi_stats_overflow(const CetStats* s);
size_t    cet_ffi_stats_max_depth_seen(const CetStats* s);

/* Runtime */
CetRuntimeConfig* cet_ffi_runtime_config_new(void);
void              cet_ffi_runtime_config_free(CetRuntimeConfig* c);
void              cet_ffi_runtime_config_set_native_threads(CetRuntimeConfig* c, size_t n);

CetRuntimeStats*  cet_ffi_runtime_stats_new(void);
void              cet_ffi_runtime_stats_free(CetRuntimeStats* s);
size_t            cet_ffi_runtime_stats_native_threads_used(const CetRuntimeStats* s);
int               cet_ffi_runtime_stats_parallel_enabled(const CetRuntimeStats* s);

/* Execute */
int cet_ffi_execute_mcet(const CetGraph* g, const CetQuery* q,
                         CetResult* out, CetStats* stats);
int cet_ffi_execute_tcet(const CetGraph* g, const CetQuery* q,
                         CetResult* out, CetStats* stats);
int cet_ffi_execute_hcet(const CetGraph* g, const CetQuery* q,
                         size_t switch_depth,
                         CetResult* out, CetStats* stats);
int cet_ffi_execute_hcet_parallel(const CetGraph* g, const CetQuery* q,
                                  size_t switch_depth,
                                  const CetRuntimeConfig* cfg,
                                  CetResult* out, CetStats* stats,
                                  CetRuntimeStats* rt);

#ifdef __cplusplus
}
#endif

#endif /* CET_FFI_H */
