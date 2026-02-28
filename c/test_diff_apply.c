#define _POSIX_C_SOURCE 200809L
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* MSVC uses _strdup; POSIX uses strdup */
#ifdef _MSC_VER
#  define strdup _strdup
#endif

#include "diff_apply.h"

/* ═══════════════════════════════════════════════════════════════════════
 *  Minimal test runner
 * ═══════════════════════════════════════════════════════════════════════ */

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
 *  GenericValue helpers (test-local constructors)
 * ═══════════════════════════════════════════════════════════════════════ */

static GenericValue *gv_str(const char *s) {
    GenericValue *g = calloc(1, sizeof *g);
    g->kind   = GV_STRING;
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

static GenericValue *gv_arr_new(void) {
    GenericValue *g = calloc(1, sizeof *g);
    g->kind = GV_ARRAY;
    return g;
}

static void gv_arr_push(GenericValue *a, GenericValue *v) {
    if (a->arr.len == a->arr.cap) {
        size_t cap = a->arr.cap ? a->arr.cap * 2 : 4;
        a->arr.items = realloc(a->arr.items, cap * sizeof(GenericValue *));
        a->arr.cap   = cap;
    }
    a->arr.items[a->arr.len++] = v;
}

/* Order-insensitive deep equality (maps compared by key, not position). */
static int gv_eq(const GenericValue *a, const GenericValue *b) {
    if (!a || !b) return a == b;
    if (a->kind != b->kind) return 0;
    switch (a->kind) {
        case GV_NULL:    return 1;
        case GV_BOOL:    return a->boolean == b->boolean;
        case GV_STRING:
        case GV_NUMERIC: return strcmp(a->string, b->string) == 0;
        case GV_ARRAY:
            if (a->arr.len != b->arr.len) return 0;
            for (size_t i = 0; i < a->arr.len; i++)
                if (!gv_eq(a->arr.items[i], b->arr.items[i])) return 0;
            return 1;
        case GV_MAP:
            if (a->map.len != b->map.len) return 0;
            for (size_t i = 0; i < a->map.len; i++) {
                int found = 0;
                for (size_t j = 0; j < b->map.len; j++) {
                    if (strcmp(a->map.ents[i].key, b->map.ents[j].key) == 0
                        && gv_eq(a->map.ents[i].val, b->map.ents[j].val)) {
                        found = 1; break;
                    }
                }
                if (!found) return 0;
            }
            return 1;
    }
    return 0;
}

/* Shorthand: apply diff to a string GV, assert no error. */
static void apply_txt(const DiffDoc__Mismatches *d, GenericValue *doc) {
    DocError errs[4];
    size_t   nerrs = 0;
    int rc = mismatches_apply_mut(d, doc, 0, errs, 4, &nerrs);
    if (rc != 0 || nerrs != 0)
        fprintf(stderr, "  apply_txt error: %s\n",
                nerrs ? errs[0].msg : "(unknown)");
}

/* Shorthand: apply diff to a doc GV, assert no error. */
static void apply_doc(const DiffDoc__Mismatches *d, GenericValue *doc) {
    DocError errs[4];
    size_t   nerrs = 0;
    int rc = mismatches_apply_mut(d, doc, 0, errs, 4, &nerrs);
    if (rc != 0 || nerrs != 0)
        fprintf(stderr, "  apply_doc error: %s\n",
                nerrs ? errs[0].msg : "(unknown)");
}

/* ═══════════════════════════════════════════════════════════════════════
 *  txt tests  (mirrors tests/txt/mod.rs — apply commutativity)
 * ═══════════════════════════════════════════════════════════════════════ */

/*
 * txt case 1
 *   base   = "base text file case1\nline\nanother line\n"
 *   a      = "base text file case1\nline change\nanother line\n"
 *   b      = "base text file case1\nline\nanother line change\n"
 *   result = "base text file case1\nline change\nanother line change\n"
 *
 *   pa (base→a): Append{index=1, pos=4,  value=" change"}
 *   pb (base→b): Append{index=2, pos=12, value=" change"}
 */
static void test_txt_case1(void) {
    const char *BASE   = "base text file case1\nline\nanother line\n";
    const char *RESULT = "base text file case1\nline change\nanother line change\n";

    /* pa */
    DiffDoc__DiffOpAppend pa_app = {0}; pa_app.index = 1; pa_app.pos = 4; pa_app.value = " change";
    DiffDoc__DiffOp       pa_op  = {0}; pa_op.kind_case = DIFF_DOC__DIFF_OP__KIND_APPEND; pa_op.append = &pa_app;
    DiffDoc__DiffOp      *pa_ops[] = {&pa_op};
    DiffDoc__TxtMismatch  pa_tm = {0}; pa_tm.n_ops = 1; pa_tm.ops = pa_ops;
    DiffDoc__Mismatches   pa    = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pa.text = &pa_tm;

    /* pb */
    DiffDoc__DiffOpAppend pb_app = {0}; pb_app.index = 2; pb_app.pos = 12; pb_app.value = " change";
    DiffDoc__DiffOp       pb_op  = {0}; pb_op.kind_case = DIFF_DOC__DIFF_OP__KIND_APPEND; pb_op.append = &pb_app;
    DiffDoc__DiffOp      *pb_ops[] = {&pb_op};
    DiffDoc__TxtMismatch  pb_tm = {0}; pb_tm.n_ops = 1; pb_tm.ops = pb_ops;
    DiffDoc__Mismatches   pb    = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pb.text = &pb_tm;

    /* pa.apply(pb.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pb, &doc);
        apply_txt(&pa, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }

    /* pb.apply(pa.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pa, &doc);
        apply_txt(&pb, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }
}

/*
 * txt case 2
 *   base   = "base text file case2\nline\nanother line"
 *   a      = "base text file case2\nline change\nanother line"
 *   b      = "base text file case2\nline\nanother line\nnew line\n"
 *   result = "base text file case2\nline change\nanother line\nnew line\n"
 *
 *   pa (base→a): Append{index=1, pos=4, value=" change"}
 *   pb (base→b): Insert{index=3, value="new line"}, Insert{index=4, value=""}
 */
static void test_txt_case2(void) {
    const char *BASE   = "base text file case2\nline\nanother line";
    const char *RESULT = "base text file case2\nline change\nanother line\nnew line\n";

    /* pa */
    DiffDoc__DiffOpAppend pa_app = {0}; pa_app.index = 1; pa_app.pos = 4; pa_app.value = " change";
    DiffDoc__DiffOp       pa_op  = {0}; pa_op.kind_case = DIFF_DOC__DIFF_OP__KIND_APPEND; pa_op.append = &pa_app;
    DiffDoc__DiffOp      *pa_ops[] = {&pa_op};
    DiffDoc__TxtMismatch  pa_tm = {0}; pa_tm.n_ops = 1; pa_tm.ops = pa_ops;
    DiffDoc__Mismatches   pa    = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pa.text = &pa_tm;

    /* pb: two inserts */
    DiffDoc__DiffOpInsert pb_ins1 = {0}; pb_ins1.index = 3; pb_ins1.value = "new line";
    DiffDoc__DiffOpInsert pb_ins2 = {0}; pb_ins2.index = 4; pb_ins2.value = "";
    DiffDoc__DiffOp       pb_op1  = {0}; pb_op1.kind_case = DIFF_DOC__DIFF_OP__KIND_INSERT; pb_op1.insert = &pb_ins1;
    DiffDoc__DiffOp       pb_op2  = {0}; pb_op2.kind_case = DIFF_DOC__DIFF_OP__KIND_INSERT; pb_op2.insert = &pb_ins2;
    DiffDoc__DiffOp      *pb_ops[] = {&pb_op1, &pb_op2};
    DiffDoc__TxtMismatch  pb_tm = {0}; pb_tm.n_ops = 2; pb_tm.ops = pb_ops;
    DiffDoc__Mismatches   pb    = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pb.text = &pb_tm;

    /* pa.apply(pb.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pb, &doc);
        apply_txt(&pa, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }

    /* pb.apply(pa.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pa, &doc);
        apply_txt(&pb, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }
}

/*
 * txt case 3
 *   base   = "base text file case3\nline\nanother line\ndeleted line\nfinal line\nend of file\n"
 *   a      = "base text file case3\nline\nanother line\nfinal line\nend of file\n"
 *   b      = "base text file case3\nline change\nanother line\ndeleted line\nfinal line\nend of file\n"
 *   result = "base text file case3\nline change\nanother line\nfinal line\nend of file\n"
 *
 *   pa (base→a): Remove{index=3}
 *   pb (base→b): Append{index=1, pos=4, value=" change"}
 */
static void test_txt_case3(void) {
    const char *BASE   = "base text file case3\nline\nanother line\ndeleted line\nfinal line\nend of file\n";
    const char *RESULT = "base text file case3\nline change\nanother line\nfinal line\nend of file\n";

    /* pa */
    DiffDoc__DiffOpRemove pa_rem = {0}; pa_rem.index = 3;
    DiffDoc__DiffOp       pa_op  = {0}; pa_op.kind_case = DIFF_DOC__DIFF_OP__KIND_REMOVE; pa_op.remove = &pa_rem;
    DiffDoc__DiffOp      *pa_ops[] = {&pa_op};
    DiffDoc__TxtMismatch  pa_tm = {0}; pa_tm.n_ops = 1; pa_tm.ops = pa_ops;
    DiffDoc__Mismatches   pa    = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pa.text = &pa_tm;

    /* pb */
    DiffDoc__DiffOpAppend pb_app = {0}; pb_app.index = 1; pb_app.pos = 4; pb_app.value = " change";
    DiffDoc__DiffOp       pb_op  = {0}; pb_op.kind_case = DIFF_DOC__DIFF_OP__KIND_APPEND; pb_op.append = &pb_app;
    DiffDoc__DiffOp      *pb_ops[] = {&pb_op};
    DiffDoc__TxtMismatch  pb_tm = {0}; pb_tm.n_ops = 1; pb_tm.ops = pb_ops;
    DiffDoc__Mismatches   pb    = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_TEXT; pb.text = &pb_tm;

    /* pa.apply(pb.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pb, &doc);
        apply_txt(&pa, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }

    /* pb.apply(pa.apply(base)) == result */
    {
        GenericValue doc = {0}; doc.kind = GV_STRING; doc.string = strdup(BASE);
        apply_txt(&pa, &doc);
        apply_txt(&pb, &doc);
        ASSERT_STR_EQ(doc.string, RESULT);
        free(doc.string);
    }
}

/* ═══════════════════════════════════════════════════════════════════════
 *  doc tests  (mirrors tests/json/mod.rs — apply commutativity)
 * ═══════════════════════════════════════════════════════════════════════ */

/* Wrap a char* in a proto GenericValue (stack, caller ensures lifetime). */
static DiffDoc__GenericValue proto_str_val(char *s) {
    DiffDoc__GenericValue v = {0};
    v.kind_case = DIFF_DOC__GENERIC_VALUE__KIND_STRING;
    v.string    = s;
    return v;
}

/*
 * doc case 1: map — two independent key updates
 *   base   = {"base":"json file case1","line":"base","another":"line"}
 *   a      = {"base":"json file case1","line":"changed","another":"line"}
 *   b      = {"base":"json file case1","line":"base","another":"changed"}
 *   result = {"base":"json file case1","line":"changed","another":"changed"}
 *
 *   pa: Hunk{path:[Name("line")],    Update("changed")}
 *   pb: Hunk{path:[Name("another")], Update("changed")}
 */
static void test_doc_case1(void) {
    DiffDoc__GenericValue gv_changed = proto_str_val("changed");

    /* pa */
    DiffDoc__DocIndex    pa_i0   = {0}; pa_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; pa_i0.name = "line";
    DiffDoc__DocIndex   *pa_path[] = {&pa_i0};
    DiffDoc__HunkAction  pa_act  = {0}; pa_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pa_act.update = &gv_changed;
    DiffDoc__Hunk        pa_h    = {0}; pa_h.n_path = 1; pa_h.path = pa_path; pa_h.value = &pa_act;
    DiffDoc__Hunk       *pa_hs[] = {&pa_h};
    DiffDoc__DocMismatch pa_dm   = {0}; pa_dm.n_hunks = 1; pa_dm.hunks = pa_hs;
    DiffDoc__Mismatches  pa      = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pa.doc = &pa_dm;

    /* pb */
    DiffDoc__DocIndex    pb_i0   = {0}; pb_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; pb_i0.name = "another";
    DiffDoc__DocIndex   *pb_path[] = {&pb_i0};
    DiffDoc__HunkAction  pb_act  = {0}; pb_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pb_act.update = &gv_changed;
    DiffDoc__Hunk        pb_h    = {0}; pb_h.n_path = 1; pb_h.path = pb_path; pb_h.value = &pb_act;
    DiffDoc__Hunk       *pb_hs[] = {&pb_h};
    DiffDoc__DocMismatch pb_dm   = {0}; pb_dm.n_hunks = 1; pb_dm.hunks = pb_hs;
    DiffDoc__Mismatches  pb      = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pb.doc = &pb_dm;

    GenericValue *expected = gv_map_new();
    gv_map_put(expected, "base",    gv_str("json file case1"));
    gv_map_put(expected, "line",    gv_str("changed"));
    gv_map_put(expected, "another", gv_str("changed"));

    /* base + pa + pb */
    {
        GenericValue *base = gv_map_new();
        gv_map_put(base, "base", gv_str("json file case1"));
        gv_map_put(base, "line", gv_str("base"));
        gv_map_put(base, "another", gv_str("line"));
        apply_doc(&pa, base); apply_doc(&pb, base);
        ASSERT(gv_eq(base, expected), "doc case1: base+pa+pb == result"); gv_free(base);
    }

    /* base + pb + pa */
    {
        GenericValue *base = gv_map_new();
        gv_map_put(base, "base", gv_str("json file case1"));
        gv_map_put(base, "line", gv_str("base"));
        gv_map_put(base, "another", gv_str("line"));
        apply_doc(&pb, base); apply_doc(&pa, base);
        ASSERT(gv_eq(base, expected), "doc case1: base+pb+pa == result"); gv_free(base);
    }

    gv_free(expected);
}

/*
 * doc case 2: array — two independent index updates
 *   base   = ["json file case2","base","line"]
 *   a      = ["json file case2","changed","line"]
 *   b      = ["json file case2","base","changed"]
 *   result = ["json file case2","changed","changed"]
 *
 *   pa: Hunk{path:[Idx(1)], Update("changed")}
 *   pb: Hunk{path:[Idx(2)], Update("changed")}
 */
static void test_doc_case2(void) {
    DiffDoc__GenericValue gv_changed = proto_str_val("changed");

    /* pa */
    DiffDoc__DocIndex    pa_i0   = {0}; pa_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX; pa_i0.idx = 1;
    DiffDoc__DocIndex   *pa_path[] = {&pa_i0};
    DiffDoc__HunkAction  pa_act  = {0}; pa_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pa_act.update = &gv_changed;
    DiffDoc__Hunk        pa_h    = {0}; pa_h.n_path = 1; pa_h.path = pa_path; pa_h.value = &pa_act;
    DiffDoc__Hunk       *pa_hs[] = {&pa_h};
    DiffDoc__DocMismatch pa_dm   = {0}; pa_dm.n_hunks = 1; pa_dm.hunks = pa_hs;
    DiffDoc__Mismatches  pa      = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pa.doc = &pa_dm;

    /* pb */
    DiffDoc__DocIndex    pb_i0   = {0}; pb_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX; pb_i0.idx = 2;
    DiffDoc__DocIndex   *pb_path[] = {&pb_i0};
    DiffDoc__HunkAction  pb_act  = {0}; pb_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pb_act.update = &gv_changed;
    DiffDoc__Hunk        pb_h    = {0}; pb_h.n_path = 1; pb_h.path = pb_path; pb_h.value = &pb_act;
    DiffDoc__Hunk       *pb_hs[] = {&pb_h};
    DiffDoc__DocMismatch pb_dm   = {0}; pb_dm.n_hunks = 1; pb_dm.hunks = pb_hs;
    DiffDoc__Mismatches  pb      = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pb.doc = &pb_dm;

    GenericValue *expected = gv_arr_new();
    gv_arr_push(expected, gv_str("json file case2"));
    gv_arr_push(expected, gv_str("changed"));
    gv_arr_push(expected, gv_str("changed"));

    /* base + pa + pb */
    {
        GenericValue *base = gv_arr_new();
        gv_arr_push(base, gv_str("json file case2"));
        gv_arr_push(base, gv_str("base"));
        gv_arr_push(base, gv_str("line"));
        apply_doc(&pa, base); apply_doc(&pb, base);
        ASSERT(gv_eq(base, expected), "doc case2: base+pa+pb == result"); gv_free(base);
    }

    /* base + pb + pa */
    {
        GenericValue *base = gv_arr_new();
        gv_arr_push(base, gv_str("json file case2"));
        gv_arr_push(base, gv_str("base"));
        gv_arr_push(base, gv_str("line"));
        apply_doc(&pb, base); apply_doc(&pa, base);
        ASSERT(gv_eq(base, expected), "doc case2: base+pb+pa == result"); gv_free(base);
    }

    gv_free(expected);
}

/*
 * doc case 3: array of maps — update nested field + remove element
 *   base   = [{"name":"json file case3"},{"name":"base"},{"name":"line"},
 *              {"name":"to delete"},{"name":"the end"}]
 *   a      = [{"name":"json file case3"},{"name":"base changed"},{"name":"line"},
 *              {"name":"to delete"},{"name":"the end"}]
 *   b      = [{"name":"json file case3"},{"name":"base"},{"name":"changed"},
 *              {"name":"the end"}]
 *   result = [{"name":"json file case3"},{"name":"base changed"},{"name":"changed"},
 *              {"name":"the end"}]
 *
 *   pa: Hunk{path:[Idx(1), Name("name")], Update("base changed")}
 *   pb: Hunk{path:[Idx(2), Name("name")], Update("changed")}
 *       Hunk{path:[Idx(3)],               Remove}
 */
static void test_doc_case3(void) {
    DiffDoc__GenericValue gv_base_changed = proto_str_val("base changed");
    DiffDoc__GenericValue gv_changed      = proto_str_val("changed");

    /* pa: [Idx(1), Name("name")] → Update("base changed") */
    DiffDoc__DocIndex    pa_i0 = {0}; pa_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX;  pa_i0.idx  = 1;
    DiffDoc__DocIndex    pa_i1 = {0}; pa_i1.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; pa_i1.name = "name";
    DiffDoc__DocIndex   *pa_path[] = {&pa_i0, &pa_i1};
    DiffDoc__HunkAction  pa_act = {0}; pa_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pa_act.update = &gv_base_changed;
    DiffDoc__Hunk        pa_h   = {0}; pa_h.n_path = 2; pa_h.path = pa_path; pa_h.value = &pa_act;
    DiffDoc__Hunk       *pa_hs[] = {&pa_h};
    DiffDoc__DocMismatch pa_dm  = {0}; pa_dm.n_hunks = 1; pa_dm.hunks = pa_hs;
    DiffDoc__Mismatches  pa     = {0}; pa.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pa.doc = &pa_dm;

    /* pb hunk 1: [Idx(2), Name("name")] → Update("changed") */
    DiffDoc__DocIndex    pb1_i0 = {0}; pb1_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX;  pb1_i0.idx  = 2;
    DiffDoc__DocIndex    pb1_i1 = {0}; pb1_i1.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; pb1_i1.name = "name";
    DiffDoc__DocIndex   *pb1_path[] = {&pb1_i0, &pb1_i1};
    DiffDoc__HunkAction  pb1_act = {0}; pb1_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; pb1_act.update = &gv_changed;
    DiffDoc__Hunk        pb_h1   = {0}; pb_h1.n_path = 2; pb_h1.path = pb1_path; pb_h1.value = &pb1_act;

    /* pb hunk 2: [Idx(3)] → Remove */
    DiffDoc__DocIndex    pb2_i0 = {0}; pb2_i0.kind_case = DIFF_DOC__DOC_INDEX__KIND_IDX; pb2_i0.idx = 3;
    DiffDoc__DocIndex   *pb2_path[] = {&pb2_i0};
    DiffDoc__HunkAction  pb2_act = {0}; pb2_act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_REMOVE;
    DiffDoc__Hunk        pb_h2   = {0}; pb_h2.n_path = 1; pb_h2.path = pb2_path; pb_h2.value = &pb2_act;

    DiffDoc__Hunk       *pb_hs[] = {&pb_h1, &pb_h2};
    DiffDoc__DocMismatch pb_dm   = {0}; pb_dm.n_hunks = 2; pb_dm.hunks = pb_hs;
    DiffDoc__Mismatches  pb      = {0}; pb.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; pb.doc = &pb_dm;

    GenericValue *expected = gv_arr_new();
    { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("json file case3")); gv_arr_push(expected, m); }
    { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("base changed"));   gv_arr_push(expected, m); }
    { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("changed"));        gv_arr_push(expected, m); }
    { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("the end"));        gv_arr_push(expected, m); }

    /* base + pa + pb */
    {
        GenericValue *base = gv_arr_new();
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("json file case3")); gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("base"));            gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("line"));            gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("to delete"));      gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("the end"));        gv_arr_push(base, m); }
        apply_doc(&pa, base); apply_doc(&pb, base);
        ASSERT(gv_eq(base, expected), "doc case3: base+pa+pb == result"); gv_free(base);
    }

    /* base + pb + pa */
    {
        GenericValue *base = gv_arr_new();
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("json file case3")); gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("base"));            gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("line"));            gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("to delete"));      gv_arr_push(base, m); }
        { GenericValue *m = gv_map_new(); gv_map_put(m, "name", gv_str("the end"));        gv_arr_push(base, m); }
        apply_doc(&pb, base); apply_doc(&pa, base);
        ASSERT(gv_eq(base, expected), "doc case3: base+pb+pa == result"); gv_free(base);
    }

    gv_free(expected);
}

/* ═══════════════════════════════════════════════════════════════════════
 *  CoW (mismatches_apply) smoke test
 * ═══════════════════════════════════════════════════════════════════════ */
static void test_cow_apply(void) {
    DiffDoc__GenericValue gv_v = proto_str_val("new");
    DiffDoc__DocIndex     idx  = {0}; idx.kind_case = DIFF_DOC__DOC_INDEX__KIND_NAME; idx.name = "k";
    DiffDoc__DocIndex    *path[] = {&idx};
    DiffDoc__HunkAction   act  = {0}; act.kind_case = DIFF_DOC__HUNK_ACTION__KIND_UPDATE; act.update = &gv_v;
    DiffDoc__Hunk         h    = {0}; h.n_path = 1; h.path = path; h.value = &act;
    DiffDoc__Hunk        *hs[] = {&h};
    DiffDoc__DocMismatch  dm   = {0}; dm.n_hunks = 1; dm.hunks = hs;
    DiffDoc__Mismatches   diff = {0}; diff.kind_case = DIFF_DOC__MISMATCHES__KIND_DOC; diff.doc = &dm;

    GenericValue *original = gv_map_new();
    gv_map_put(original, "k", gv_str("old"));

    DocError err = {NULL};
    GenericValue *result = mismatches_apply(&diff, original, &err);

    ASSERT(result != NULL,                         "cow: returns non-null");
    ASSERT(err.msg == NULL,                        "cow: no error");

    /* original is unchanged */
    ASSERT(strcmp(original->map.ents[0].val->string, "old") == 0, "cow: original unchanged");

    /* result has the new value */
    ASSERT(result->kind == GV_MAP,                 "cow: result is map");
    ASSERT(strcmp(result->map.ents[0].val->string, "new") == 0,   "cow: result updated");

    gv_free(original);
    gv_free(result);
}

/* ═══════════════════════════════════════════════════════════════════════
 *  main
 * ═══════════════════════════════════════════════════════════════════════ */
int main(void) {
    printf("--- txt tests ---\n");
    test_txt_case1();
    test_txt_case2();
    test_txt_case3();

    printf("--- doc tests ---\n");
    test_doc_case1();
    test_doc_case2();
    test_doc_case3();

    printf("--- cow test ---\n");
    test_cow_apply();

    printf("\n%d passed, %d failed\n", g_pass, g_fail);
    return g_fail ? 1 : 0;
}
