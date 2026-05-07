#include "cet.h"

size_t cet_materialize_windows(int64_t start, int64_t end, int64_t within, int64_t slide, int64_t out[][2], size_t cap){
  size_t n=0;
  for(int64_t t=start; t+within<=end && n<cap; t+=slide){
    out[n][0]=t;
    out[n][1]=t+within;
    n++;
  }
  return n;
}
