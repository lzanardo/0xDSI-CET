#include "cet.h"
#include <string.h>
#include <stdio.h>
#include <stdlib.h>

static double G_MEM_V=0.7, G_MEM_E=0.3, G_CPU_E=0.8, G_CPU_V=0.2;
void cet_set_cost_coefficients(double mem_vertex, double mem_edge, double cpu_edge, double cpu_vertex){
  G_MEM_V=mem_vertex; G_MEM_E=mem_edge; G_CPU_E=cpu_edge; G_CPU_V=cpu_vertex;
}

void cet_estimate_costs(cet_graphlet_t* arr, size_t n){
  for(size_t i=0;i<n;i++){
    arr[i].memory_cost = arr[i].vertex_count * G_MEM_V + arr[i].edge_count * G_MEM_E;
    arr[i].cpu_cost = arr[i].edge_count * G_CPU_E + arr[i].vertex_count * G_CPU_V;
  }
}

size_t cet_detect_graphlets(int64_t windows[][2], size_t nwin, cet_graphlet_t* out, size_t cap){
  size_t n = nwin<cap?nwin:cap;
  for(size_t i=0;i<n;i++){
    snprintf(out[i].graphlet_id,sizeof(out[i].graphlet_id),"g%zu",i);
    out[i].start_ms=windows[i][0]; out[i].end_ms=windows[i][1];
    out[i].vertex_count=(int)((windows[i][1]-windows[i][0])/10)+1;
    out[i].edge_count=(int)((windows[i][1]-windows[i][0])/8)+1;
  }
  cet_estimate_costs(out,n);
  return n;
}

static int same_graphlet(const cet_graphlet_t* a, const cet_graphlet_t* b){ return a->start_ms==b->start_ms && a->end_ms==b->end_ms; }
void cet_classify_graphlet_delta(const cet_graphlet_t* prev, size_t pcount, const cet_graphlet_t* curr, size_t ccount, cet_graphlet_delta_t* out){
  memset(out,0,sizeof(*out));
  for(size_t i=0;i<ccount;i++){ int found=0; for(size_t j=0;j<pcount;j++) if(same_graphlet(&curr[i],&prev[j])){ found=1; break; } if(found) out->shared_idx[out->shared_count++]=i; else out->new_idx[out->new_count++]=i; }
  for(size_t j=0;j<pcount;j++){ int found=0; for(size_t i=0;i<ccount;i++) if(same_graphlet(&prev[j],&curr[i])){ found=1; break; } if(!found) out->expired_idx[out->expired_count++]=j; }
}

void cet_partial_cache_init(cet_partial_cache_t* cache){ memset(cache,0,sizeof(*cache)); }
void cet_partial_cache_touch(cet_partial_cache_t* cache, int key_vertex, size_t seq_idx){
  for(size_t i=0;i<cache->count;i++) if(cache->entries[i].key_vertex==key_vertex && cache->entries[i].seq_idx==seq_idx){ cache->entries[i].hits++; return; }
  if(cache->count<CET_MAX_GRAPHLETS){ cache->entries[cache->count].key_vertex=key_vertex; cache->entries[cache->count].seq_idx=seq_idx; cache->entries[cache->count].hits=1; cache->count++; }
}

void cet_greedy_plan(const cet_graphlet_t* gl, size_t n, double max_mem, cet_plan_t* out){
  memset(out,0,sizeof(*out)); int used[CET_MAX_GRAPHLETS]={0};
  while(1){ int best=-1; double bestv=1e100; for(size_t i=0;i<n;i++) if(!used[i]){ if(out->total_memory + gl[i].memory_cost > max_mem) continue; double score=gl[i].cpu_cost+gl[i].memory_cost; if(score<bestv){best=(int)i;bestv=score;} } if(best<0) break; used[best]=1; out->indices[out->count++]=best; out->total_memory += gl[best].memory_cost; out->total_cpu += gl[best].cpu_cost; }
}

static void bnb_rec(const cet_graphlet_t* gl,size_t n,size_t i,double max_mem,cet_plan_t* cur,cet_plan_t* best){
  if(cur->total_memory>max_mem || cur->total_cpu>=best->total_cpu) return;
  if(i==n){ if(cur->total_cpu<best->total_cpu && cur->count>0) *best=*cur; return; }
  bnb_rec(gl,n,i+1,max_mem,cur,best);
  if(cur->count<CET_MAX_GRAPHLETS){ cur->indices[cur->count++]=i; cur->total_memory += gl[i].memory_cost; cur->total_cpu += gl[i].cpu_cost; bnb_rec(gl,n,i+1,max_mem,cur,best); cur->count--; cur->total_memory -= gl[i].memory_cost; cur->total_cpu -= gl[i].cpu_cost; }
}

void cet_branch_and_bound_plan(const cet_graphlet_t* gl, size_t n, double max_mem, cet_plan_t* out){ cet_plan_t cur; memset(&cur,0,sizeof(cur)); memset(out,0,sizeof(*out)); out->total_cpu=1e100; bnb_rec(gl,n,0,max_mem,&cur,out); if(out->total_cpu==1e100) memset(out,0,sizeof(*out)); }
