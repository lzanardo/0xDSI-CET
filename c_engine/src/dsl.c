#include "cet.h"
#include <string.h>
#include <stdio.h>
#include <ctype.h>

static void trim(char* s){
  while(*s && isspace((unsigned char)*s)) memmove(s,s+1,strlen(s));
  size_t n=strlen(s);
  while(n>0 && isspace((unsigned char)s[n-1])) s[--n]='\0';
}

int cet_parse_query(const char* name, const char* pattern_csv, int64_t within_ms, int64_t slide_ms, cet_query_t* out) {
  memset(out, 0, sizeof(*out));
  snprintf(out->name, sizeof(out->name), "%s", name);
  out->within_ms = within_ms;
  out->slide_ms = slide_ms;
  out->skip_till_any_match = 1;

  char buf[1024];
  snprintf(buf, sizeof(buf), "%s", pattern_csv);
  char* tok = strtok(buf, ",");
  while (tok && out->seq_len < CET_MAX_SEQ) {
    trim(tok);
    cet_event_type_t* e = &out->seq[out->seq_len++];
    size_t n = strlen(tok);
    if (n > 0 && tok[n-1] == '+') {
      e->kleene_plus = 1;
      tok[n-1] = '\0';
      trim(tok);
    }
    snprintf(e->name, sizeof(e->name), "%s", tok);
    tok = strtok(NULL, ",");
  }
  return out->seq_len > 0 ? 0 : -1;
}
