#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _MSC_VER
#  define strdup _strdup
#endif

#include "bson_doc.h"

/*
 * bson_doc.c against src/bson_doc.rs: testdata/types.bson is the BSON the Rust side
 * writes of every type, types.txt the GenericValue it reads it as (the_c_fixture,
 * BSON_FIXTURE=write cargo test --features bson). Run from c/.
 */

static int g_pass = 0, g_fail = 0;

#define ASSERT(cond, msg) \
    do { \
        if (cond) { \
            g_pass++; \
        } else { \
            fprintf(stderr, "FAIL %s:%d  %s\n", __FILE__, __LINE__, (msg)); \
            g_fail++; \
        } \
    } while (0)

#define ASSERT_STR_EQ(a, b) \
    do { \
        if (strcmp((a), (b)) == 0) { \
            g_pass++; \
        } else { \
            fprintf(stderr, "FAIL %s:%d\n  got : %s\n  want: %s\n", \
                    __FILE__, __LINE__, (a), (b)); \
            g_fail++; \
        } \
    } while (0)

/* ═══════════════════════════════════════════════════════════════════════
 *  Helpers
 * ═══════════════════════════════════════════════════════════════════════ */

static uint8_t *read_file(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    if (!f) return NULL;
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *b = malloc((size_t)n + 1);
    *len = fread(b, 1, (size_t)n, f);
    b[*len] = 0;
    fclose(f);
    return b;
}

typedef struct { char *p; size_t len, cap; } Str;

static void str_put(Str *s, const char *t, size_t n) {
    if (s->len + n + 1 > s->cap) {
        s->cap = (s->len + n + 1) * 2;
        s->p = realloc(s->p, s->cap);
    }
    memcpy(s->p + s->len, t, n);
    s->len += n;
    s->p[s->len] = 0;
}

static void str_cat(Str *s, const char *t) { str_put(s, t, strlen(t)); }

/* Rust's {:?} of a str */
static void quoted(Str *s, const char *t) {
    str_cat(s, "\"");
    for (; *t; t++) {
        char buf[16];
        switch (*t) {
            case '"':  str_cat(s, "\\\""); break;
            case '\\': str_cat(s, "\\\\"); break;
            case '\n': str_cat(s, "\\n"); break;
            case '\r': str_cat(s, "\\r"); break;
            case '\t': str_cat(s, "\\t"); break;
            default:
                if ((unsigned char)*t < 0x20) { snprintf(buf, sizeof buf, "\\u{%x}", *t); str_cat(s, buf); }
                else str_put(s, t, 1);
        }
    }
    str_cat(s, "\"");
}

static int cmp_ent(const void *a, const void *b) {
    return strcmp((*(const MapEntry *const *)a)->key, (*(const MapEntry *const *)b)->key);
}

/* the dump of the Rust test's: kind, value, map keys sorted */
static void dump(Str *s, const GenericValue *v) {
    switch (v->kind) {
        case GV_NULL:    str_cat(s, "z"); break;
        case GV_BOOL:    str_cat(s, v->boolean ? "btrue" : "bfalse"); break;
        case GV_NUMERIC: str_cat(s, "n"); quoted(s, v->string); break;
        case GV_STRING:  str_cat(s, "s"); quoted(s, v->string); break;
        case GV_ARRAY:
            str_cat(s, "[");
            for (size_t i = 0; i < v->arr.len; i++) {
                if (i) str_cat(s, ",");
                dump(s, v->arr.items[i]);
            }
            str_cat(s, "]");
            break;
        case GV_MAP: {
            const MapEntry **e = malloc((v->map.len + 1) * sizeof *e);
            for (size_t i = 0; i < v->map.len; i++) e[i] = &v->map.ents[i];
            qsort(e, v->map.len, sizeof *e, cmp_ent);
            str_cat(s, "{");
            for (size_t i = 0; i < v->map.len; i++) {
                if (i) str_cat(s, ",");
                quoted(s, e[i]->key);
                str_cat(s, ":");
                dump(s, e[i]->val);
            }
            str_cat(s, "}");
            free(e);
            break;
        }
    }
}

static char *dump_str(const GenericValue *v) {
    Str s = {0};
    str_cat(&s, "");
    dump(&s, v);
    return s.p;
}

static GenericValue *gv_text(GvKind k, const char *s) {
    GenericValue *g = calloc(1, sizeof *g);
    g->kind = k;
    g->string = strdup(s);
    return g;
}

static GenericValue *gv_map_new(void) {
    GenericValue *g = calloc(1, sizeof *g);
    g->kind = GV_MAP;
    return g;
}

static void gv_map_put(GenericValue *m, const char *k, GenericValue *v) {
    if (m->map.len == m->map.cap) {
        size_t cap = m->map.cap ? m->map.cap * 2 : 4;
        m->map.ents = realloc(m->map.ents, cap * sizeof(MapEntry));
        m->map.cap  = cap;
    }
    m->map.ents[m->map.len].key = strdup(k);
    m->map.ents[m->map.len].val = v;
    m->map.len++;
}

static const GenericValue *gv_get(const GenericValue *m, const char *k) {
    for (size_t i = 0; m && m->kind == GV_MAP && i < m->map.len; i++)
        if (strcmp(m->map.ents[i].key, k) == 0) return m->map.ents[i].val;
    return NULL;
}

/* {k: v} as BSON and back: the dump of v as it reads back, or the error */
static char *round_trip(GenericValue *v) {
    GenericValue *doc = gv_map_new();
    gv_map_put(doc, "k", v);
    uint8_t *b;
    size_t n;
    DocError e;
    char *out;
    if (gv_to_bson(doc, &b, &n, &e) != 0) {
        out = strdup(e.msg);
        doc_error_free(&e);
    } else {
        GenericValue *back = gv_from_bson(b, n, &e);
        out = back ? dump_str(gv_get(back, "k")) : strdup(e.msg);
        gv_free(back);
        doc_error_free(&e);
        free(b);
    }
    gv_free(doc);
    return out;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Tests
 * ═══════════════════════════════════════════════════════════════════════ */

/* every type: read as the Rust side reads it, written back to the same bytes */
static void test_fixture(void) {
    size_t blen, tlen;
    uint8_t *bytes = read_file("testdata/types.bson", &blen);
    char *text = (char *)read_file("testdata/types.txt", &tlen);
    ASSERT(bytes && text, "fixture: testdata/types.bson and types.txt (run from c/)");
    if (!bytes || !text) return;
    while (tlen && text[tlen - 1] == '\n') text[--tlen] = 0;

    DocError e;
    GenericValue *v = gv_from_bson(bytes, blen, &e);
    ASSERT(v != NULL, e.msg ? e.msg : "fixture reads");
    if (v) {
        char *d = dump_str(v);
        ASSERT_STR_EQ(d, text);
        free(d);
        uint8_t *out;
        size_t olen;
        int rc = gv_to_bson(v, &out, &olen, &e);
        ASSERT(rc == 0, e.msg ? e.msg : "fixture writes");
        if (rc == 0) {
            ASSERT(olen == blen && memcmp(out, bytes, blen) == 0, "fixture: written back to the same bytes");
            free(out);
        }
        gv_free(v);
    }
    doc_error_free(&e);
    free(bytes);
    free(text);
}

/* a patch the Rust side makes, applied to a BSON document: a date changed, a key removed */
static void test_apply_patch(void) {
    size_t blen;
    uint8_t *bytes = read_file("testdata/types.bson", &blen);
    if (!bytes) return;
    DocError e;
    GenericValue *doc = gv_from_bson(bytes, blen, &e);

    DiffDoc__GenericValue date = {0};
    date.kind_case = DIFF_DOC__GENERIC_VALUE__KIND_STRING;
    date.string = "2025-01-02T03:04:05.6Z";
    DiffDoc__DocIndex i0 = {0}; i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; i0.name = "at";
    DiffDoc__DocIndex i1 = {0}; i1.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX;  i1.idx = 0;
    DiffDoc__DocIndex i2 = {0}; i2.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; i2.name = "$date";
    DiffDoc__DocIndex *p1[] = {&i0, &i1, &i2};
    DiffDoc__HunkAction a1 = {0}; a1.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; a1.update = &date;
    DiffDoc__Hunk h1 = {0}; h1.n_path = 3; h1.path = p1; h1.value = &a1;

    DiffDoc__DocIndex r0 = {0}; r0.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; r0.name = "s";
    DiffDoc__DocIndex *p2[] = {&r0};
    DiffDoc__HunkAction a2 = {0}; a2.kind_case = DIFF_DOC__HUNK_ACTION__KIND_REMOVE; a2.remove = 1;
    DiffDoc__Hunk h2 = {0}; h2.n_path = 1; h2.path = p2; h2.value = &a2;

    DiffDoc__Hunk *hs[] = {&h1, &h2};
    DiffDoc__DocMismatch dm = {0}; dm.n_hunks = 2; dm.hunks = hs;
    DiffDoc__Mismatches m = {0}; m.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; m.doc = &dm;

    DocError errs[4];
    size_t nerrs = 0;
    int rc = mismatches_apply_mut(&m, doc, 1, errs, 4, &nerrs);
    ASSERT(rc == 0 && nerrs == 0, "patch applies");

    uint8_t *out;
    size_t olen;
    ASSERT(gv_to_bson(doc, &out, &olen, &e) == 0, "patched document writes");
    GenericValue *back = gv_from_bson(out, olen, &e);
    ASSERT(back && !gv_get(back, "s"), "patched: s removed");
    const GenericValue *at = gv_get(back, "at");
    ASSERT(at && at->kind == GV_ARRAY, "patched: at");
    if (at && at->kind == GV_ARRAY)
        ASSERT_STR_EQ(gv_get(at->arr.items[0], "$date")->string, "2025-01-02T03:04:05.6Z");
    /* an _id first, as DocumentDB has it */
    ASSERT(olen > 9 && out[4] == 0x07 && memcmp(out + 5, "_id", 4) == 0, "patched: _id first");
    gv_free(back);
    free(out);
    gv_free(doc);
    free(bytes);
}

/* a number: int32, int64 or double by its value and form - none past int64 */
static void test_numbers(void) {
    struct { const char *in, *want; } cases[] = {
        {"1", "n\"1\""},
        {"-2147483649", "n\"-2147483649\""},   /* an int64 past int32's range: plain */
        {"5000000000", "n\"5000000000\""},
        {"1.5", "n\"1.5\""},
        {"-0.0", "n\"-0.0\""},
        {"1e5", "n\"100000.0\""},
        {"0.00001", "n\"1e-5\""},
        {"12345678901234567", "n\"12345678901234567\""},
        {"18446744073709551616", "DD-E502 the number 18446744073709551616 at /k has no BSON type: it is past int64"},
    };
    for (size_t i = 0; i < sizeof cases / sizeof *cases; i++) {
        char *got = round_trip(gv_text(GV_NUMERIC, cases[i].in));
        ASSERT_STR_EQ(got, cases[i].want);
        free(got);
    }
}

/* {"$numberDecimal": s} written and read back: the bson crate's forms */
static void test_decimals(void) {
    struct { const char *in, *want; } cases[] = {
        {"1.0E+3", "1.0E+3"}, {"12e-2", "0.12"}, {"-0.0", "-0.0"}, {"1E+6112", "1.0E+6112"},
        {"0E-7000", "0E-6176"}, {"+inf", "Infinity"}, {"-nan", "NaN"}, {"1.10", "1.10"},
        {"10E-6177", "1E-6176"}, {"000123", "123"}, {".5", "0.5"},
    };
    for (size_t i = 0; i < sizeof cases / sizeof *cases; i++) {
        GenericValue *m = gv_map_new();
        gv_map_put(m, "$numberDecimal", gv_text(GV_STRING, cases[i].in));
        char *got = round_trip(m);
        char want[128];
        /* a Numeric when it reads as a finite double, as from_json has it: 1.0E+6112 does not */
        int numeric = strchr("0123456789-", cases[i].want[0]) && !strstr(cases[i].want, "E+6112");
        snprintf(want, sizeof want, "{\"$numberDecimal\":%c\"%s\"}", numeric ? 'n' : 's', cases[i].want);
        ASSERT_STR_EQ(got, want);
        free(got);
    }
    const char *bad[] = {"1E-6177", "12345678901234567890123456789012345", "1x", "1e", "1E+6145"};
    for (size_t i = 0; i < sizeof bad / sizeof *bad; i++) {
        GenericValue *m = gv_map_new();
        gv_map_put(m, "$numberDecimal", gv_text(GV_STRING, bad[i]));
        char *got = round_trip(m);
        ASSERT(strncmp(got, "BSON: not a $numberDecimal", 26) == 0, bad[i]);
        free(got);
    }
}

/* what is no BSON document, or does not read */
static void test_errors(void) {
    DocError e;
    uint8_t *out;
    size_t n;
    GenericValue *arr = calloc(1, sizeof *arr);
    arr->kind = GV_ARRAY;
    ASSERT(gv_to_bson(arr, &out, &n, &e) == -1 && strncmp(e.msg, "DD-E501 ", 8) == 0, "an array is no document");
    doc_error_free(&e);
    gv_free(arr);

    GenericValue *oid = gv_map_new();
    gv_map_put(oid, "$oid", gv_text(GV_STRING, "65f1a2b3c4d5e6f708192a3b"));
    ASSERT(gv_to_bson(oid, &out, &n, &e) == -1 && strncmp(e.msg, "DD-E501 ", 8) == 0, "an ObjectId is no document");
    doc_error_free(&e);
    gv_free(oid);

    struct { GenericValue *v; const char *want; } bad[] = {
        {gv_text(GV_STRING, "nope"), "BSON: not an ObjectId at /k"},
    };
    GenericValue *m = gv_map_new();
    gv_map_put(m, "$oid", bad[0].v);
    char *got = round_trip(m);
    ASSERT_STR_EQ(got, bad[0].want);
    free(got);

    /* another $ key: a document */
    GenericValue *set = gv_map_new();
    gv_map_put(set, "$set", gv_text(GV_NUMERIC, "5"));
    got = round_trip(set);
    ASSERT_STR_EQ(got, "{\"$set\":n\"5\"}");
    free(got);

    const uint8_t truncated[] = {16, 0, 0, 0, 0x02, 'a', 0, 9, 0, 0, 0, 'x', 0};
    ASSERT(gv_from_bson(truncated, sizeof truncated, &e) == NULL && strncmp(e.msg, "BSON: ", 6) == 0, "truncated");
    doc_error_free(&e);
    const uint8_t not_utf8[] = {14, 0, 0, 0, 0x02, 'a', 0, 2, 0, 0, 0, 0xff, 0, 0};
    ASSERT(gv_from_bson(not_utf8, sizeof not_utf8, &e) == NULL && e.msg && strstr(e.msg, "UTF-8"), "not UTF-8");
    doc_error_free(&e);
    const uint8_t unknown[] = {8, 0, 0, 0, 0x20, 'a', 0, 0};
    ASSERT(gv_from_bson(unknown, sizeof unknown, &e) == NULL && e.msg && strstr(e.msg, "0x20"), "unknown type");
    doc_error_free(&e);
    const uint8_t empty[] = {5, 0, 0, 0, 0};
    GenericValue *v = gv_from_bson(empty, sizeof empty, &e);
    ASSERT(v && v->kind == GV_MAP && v->map.len == 0 && !e.msg, "an empty document");
    gv_free(v);

    /* nested past the limit */
    enum { D = 300 };
    uint8_t deep[D * 8 + 5];
    size_t at = 0;
    /* each level {"a": {...}}: 4 length, 0x03, 'a', 0 - lengths patched below */
    size_t starts[D + 1];
    for (int i = 0; i <= D; i++) {
        starts[i] = at;
        at += 4;
        if (i < D) { deep[at++] = 0x03; deep[at++] = 'a'; deep[at++] = 0; }
    }
    for (int i = D; i >= 0; i--) {
        deep[at++] = 0;
        uint32_t len = (uint32_t)(at - starts[i]);
        for (int k = 0; k < 4; k++) deep[starts[i] + k] = (uint8_t)(len >> 8 * k);
    }
    ASSERT(gv_from_bson(deep, at, &e) == NULL && e.msg && strstr(e.msg, "deeper"), "too deep");
    doc_error_free(&e);
}

int main(void) {
    printf("--- bson tests ---\n");
    test_fixture();
    test_apply_patch();
    test_numbers();
    test_decimals();
    test_errors();
    printf("\n%d passed, %d failed\n", g_pass, g_fail);
    return g_fail ? 1 : 0;
}
