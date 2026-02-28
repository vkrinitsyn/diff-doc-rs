#include <assert.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "diff_apply.h"

/* ═══════════════════════════════════════════════════════════════════════
 *  Internal helpers
 * ═══════════════════════════════════════════════════════════════════════ */

static char *str_dup(const char *s) {
    size_t n = strlen(s) + 1;
    char *d = malloc(n);
    if (d) memcpy(d, s, n);
    return d;
}

/* Insert `ins` into `s` at byte position `pos`, return new heap string. */
static char *str_insert_at(const char *s, size_t pos, const char *ins) {
    size_t slen = strlen(s);
    size_t ilen = strlen(ins);
    char *out = malloc(slen + ilen + 1);
    memcpy(out, s, pos);
    memcpy(out + pos, ins, ilen);
    memcpy(out + pos + ilen, s + pos, slen - pos + 1);
    return out;
}

static DocError make_error(const char *fmt, ...) {
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    return (DocError){ str_dup(buf) };
}

static int is_err(const DocError *e) { return e && e->msg != NULL; }

void doc_error_free(DocError *e) {
    if (e && e->msg) { free(e->msg); e->msg = NULL; }
}

/* ═══════════════════════════════════════════════════════════════════════
 *  GenericValue memory management
 * ═══════════════════════════════════════════════════════════════════════ */

void gv_free(GenericValue *gv) {
    if (!gv) return;
    switch (gv->kind) {
        case GV_STRING:
        case GV_NUMERIC:
            free(gv->string);
            break;
        case GV_ARRAY:
            for (size_t i = 0; i < gv->arr.len; i++)
                gv_free(gv->arr.items[i]);
            free(gv->arr.items);
            break;
        case GV_MAP:
            for (size_t i = 0; i < gv->map.len; i++) {
                free(gv->map.ents[i].key);
                gv_free(gv->map.ents[i].val);
            }
            free(gv->map.ents);
            break;
        default: break;
    }
    free(gv);
}

GenericValue *gv_clone(const GenericValue *src) {
    if (!src) return NULL;
    GenericValue *dst = malloc(sizeof *dst);
    if (!dst) return NULL;
    dst->kind = src->kind;
    switch (src->kind) {
        case GV_NULL:    break;
        case GV_BOOL:    dst->boolean = src->boolean; break;
        case GV_STRING:
        case GV_NUMERIC: dst->string = str_dup(src->string); break;
        case GV_ARRAY:
            dst->arr.len = src->arr.len;
            dst->arr.cap = src->arr.cap ? src->arr.cap : 1;
            dst->arr.items = malloc(dst->arr.cap * sizeof(GenericValue *));
            for (size_t i = 0; i < src->arr.len; i++)
                dst->arr.items[i] = gv_clone(src->arr.items[i]);
            break;
        case GV_MAP:
            dst->map.len = src->map.len;
            dst->map.cap = src->map.cap ? src->map.cap : 1;
            dst->map.ents = malloc(dst->map.cap * sizeof(MapEntry));
            for (size_t i = 0; i < src->map.len; i++) {
                dst->map.ents[i].key = str_dup(src->map.ents[i].key);
                dst->map.ents[i].val = gv_clone(src->map.ents[i].val);
            }
            break;
    }
    return dst;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Map helpers
 * ═══════════════════════════════════════════════════════════════════════ */

static GenericValue **map_get(GenericValue *gv, const char *key) {
    assert(gv->kind == GV_MAP);
    for (size_t i = 0; i < gv->map.len; i++)
        if (strcmp(gv->map.ents[i].key, key) == 0)
            return &gv->map.ents[i].val;
    return NULL;
}

static void map_ensure_cap(GenericValue *gv) {
    if (gv->map.len < gv->map.cap) return;
    size_t cap = gv->map.cap ? gv->map.cap * 2 : 4;
    gv->map.ents = realloc(gv->map.ents, cap * sizeof(MapEntry));
    gv->map.cap = cap;
}

/* Insert or replace key=val (takes ownership of val, frees old if present). */
static void map_set(GenericValue *gv, const char *key, GenericValue *val) {
    assert(gv->kind == GV_MAP);
    for (size_t i = 0; i < gv->map.len; i++) {
        if (strcmp(gv->map.ents[i].key, key) == 0) {
            gv_free(gv->map.ents[i].val);
            gv->map.ents[i].val = val;
            return;
        }
    }
    map_ensure_cap(gv);
    gv->map.ents[gv->map.len].key = str_dup(key);
    gv->map.ents[gv->map.len].val = val;
    gv->map.len++;
}

/* Remove key; returns the owned value (caller must free), or NULL. */
static GenericValue *map_remove(GenericValue *gv, const char *key) {
    assert(gv->kind == GV_MAP);
    for (size_t i = 0; i < gv->map.len; i++) {
        if (strcmp(gv->map.ents[i].key, key) == 0) {
            GenericValue *old = gv->map.ents[i].val;
            free(gv->map.ents[i].key);
            memmove(&gv->map.ents[i], &gv->map.ents[i + 1],
                    (gv->map.len - i - 1) * sizeof(MapEntry));
            gv->map.len--;
            return old;
        }
    }
    return NULL;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Array helpers
 * ═══════════════════════════════════════════════════════════════════════ */

static void arr_ensure_cap(GenericValue *gv, size_t need) {
    while (gv->arr.cap < need) {
        size_t cap = gv->arr.cap ? gv->arr.cap * 2 : 4;
        gv->arr.items = realloc(gv->arr.items, cap * sizeof(GenericValue *));
        gv->arr.cap = cap;
    }
}

static void arr_remove(GenericValue *gv, size_t idx) {
    assert(gv->kind == GV_ARRAY && idx < gv->arr.len);
    gv_free(gv->arr.items[idx]);
    memmove(&gv->arr.items[idx], &gv->arr.items[idx + 1],
            (gv->arr.len - idx - 1) * sizeof(GenericValue *));
    gv->arr.len--;
}

static void arr_insert(GenericValue *gv, size_t idx, GenericValue *val) {
    assert(gv->kind == GV_ARRAY && idx <= gv->arr.len);
    arr_ensure_cap(gv, gv->arr.len + 1);
    memmove(&gv->arr.items[idx + 1], &gv->arr.items[idx],
            (gv->arr.len - idx) * sizeof(GenericValue *));
    gv->arr.items[idx] = val;
    gv->arr.len++;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  proto GenericValue → C GenericValue
 *  Used when a HunkAction carries an Update/Insert value from the diff.
 * ═══════════════════════════════════════════════════════════════════════ */

static GenericValue *gv_from_proto(const DiffDoc__GenericValue *p) {
    if (!p) return NULL;
    GenericValue *gv = calloc(1, sizeof *gv);
    switch (p->kind_case) {
        case DIFF_DOC__GENERIC_VALUE__KIND_NUMERIC:
            gv->kind   = GV_NUMERIC;
            gv->string = str_dup(p->numeric);
            break;
        case DIFF_DOC__GENERIC_VALUE__KIND_STRING:
            gv->kind   = GV_STRING;
            gv->string = str_dup(p->string);
            break;
        case DIFF_DOC__GENERIC_VALUE__KIND_BOOLEAN:
            gv->kind    = GV_BOOL;
            gv->boolean = p->boolean;
            break;
        case DIFF_DOC__GENERIC_VALUE__KIND_NULL:
            gv->kind = GV_NULL;
            break;
        case DIFF_DOC__GENERIC_VALUE__KIND_ARRAY: {
            gv->kind = GV_ARRAY;
            DiffDoc__GenericArray *pa = p->array;
            size_t n = pa ? pa->n_items : 0;
            gv->arr.len = gv->arr.cap = n;
            gv->arr.items = n ? malloc(n * sizeof(GenericValue *)) : NULL;
            for (size_t i = 0; i < n; i++)
                gv->arr.items[i] = gv_from_proto(pa->items[i]);
            break;
        }
        case DIFF_DOC__GENERIC_VALUE__KIND_MAP: {
            gv->kind = GV_MAP;
            DiffDoc__GenericMap *pm = p->map;
            size_t n = pm ? pm->n_fields : 0;
            gv->map.len = gv->map.cap = n;
            gv->map.ents = n ? malloc(n * sizeof(MapEntry)) : NULL;
            for (size_t i = 0; i < n; i++) {
                gv->map.ents[i].key = str_dup(pm->fields[i]->key);
                gv->map.ents[i].val = gv_from_proto(pm->fields[i]->value);
            }
            break;
        }
        default:
            gv->kind = GV_NULL;
            break;
    }
    return gv;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  txt::Mismatch apply
 *  Mirrors txt::MismatchDocCow<String>::apply in src/txt.rs:
 *    split on '\n' → apply DiffOps → join with '\n'
 * ═══════════════════════════════════════════════════════════════════════ */

typedef struct { char **data; size_t len, cap; } Lines;

static void lines_push(Lines *l, char *s) {
    if (l->len == l->cap) {
        size_t cap = l->cap ? l->cap * 2 : 8;
        l->data = realloc(l->data, cap * sizeof(char *));
        l->cap = cap;
    }
    l->data[l->len++] = s;
}

static void lines_free(Lines *l) {
    for (size_t i = 0; i < l->len; i++) free(l->data[i]);
    free(l->data);
}

static Lines split_lines(const char *s) {
    Lines l = {NULL, 0, 0};
    for (;;) {
        const char *nl = strchr(s, '\n');
        size_t n = nl ? (size_t)(nl - s) : strlen(s);
        char *line = malloc(n + 1);
        memcpy(line, s, n);
        line[n] = '\0';
        lines_push(&l, line);
        if (!nl) break;
        s = nl + 1;
    }
    return l;
}

static char *join_lines(const Lines *l) {
    size_t total = l->len > 0 ? l->len - 1 : 0; /* separators */
    for (size_t i = 0; i < l->len; i++) total += strlen(l->data[i]);
    char *out = malloc(total + 1);
    char *p = out;
    for (size_t i = 0; i < l->len; i++) {
        size_t n = strlen(l->data[i]);
        memcpy(p, l->data[i], n);
        p += n;
        if (i + 1 < l->len) *p++ = '\n';
    }
    *p = '\0';
    return out;
}

/*
 * Apply a TxtMismatch to a '\n'-delimited string.
 * Returns a new heap string on success, NULL + *err on failure.
 * Mirrors apply_diff() in src/txt.rs.
 */
static char *txt_apply(const DiffDoc__TxtMismatch *tm,
                       const char                 *input,
                       DocError                   *err)
{
    Lines lines = split_lines(input);

    for (size_t i = 0; i < tm->n_ops; i++) {
        const DiffDoc__DiffOp *op = tm->ops[i];
        switch (op->kind_case) {

            case DIFF_DOC__DIFF_OP__KIND_REMOVE: {
                size_t idx = (size_t)op->remove->index;
                if (idx >= lines.len) {
                    *err = make_error("Remove index %zu out of bounds %zu", idx, lines.len);
                    lines_free(&lines);
                    return NULL;
                }
                free(lines.data[idx]);
                memmove(&lines.data[idx], &lines.data[idx + 1],
                        (lines.len - idx - 1) * sizeof(char *));
                lines.len--;
                break;
            }

            case DIFF_DOC__DIFF_OP__KIND_INSERT: {
                size_t idx = (size_t)op->insert->index;
                if (idx > lines.len) {
                    *err = make_error("Insert index %zu out of bounds %zu", idx, lines.len);
                    lines_free(&lines);
                    return NULL;
                }
                if (lines.len == lines.cap) {
                    size_t cap = lines.cap ? lines.cap * 2 : 8;
                    lines.data = realloc(lines.data, cap * sizeof(char *));
                    lines.cap = cap;
                }
                memmove(&lines.data[idx + 1], &lines.data[idx],
                        (lines.len - idx) * sizeof(char *));
                lines.data[idx] = str_dup(op->insert->value);
                lines.len++;
                break;
            }

            case DIFF_DOC__DIFF_OP__KIND_UPDATE: {
                size_t idx = (size_t)op->update->index;
                if (idx >= lines.len) {
                    *err = make_error("Update index %zu out of bounds %zu", idx, lines.len);
                    lines_free(&lines);
                    return NULL;
                }
                free(lines.data[idx]);
                lines.data[idx] = str_dup(op->update->value);
                break;
            }

            case DIFF_DOC__DIFF_OP__KIND_APPEND: {
                size_t idx    = (size_t)op->append->index;
                size_t pos    = (size_t)op->append->pos;
                const char *suffix = op->append->value;
                if (idx >= lines.len) {
                    *err = make_error("Append index %zu out of bounds %zu", idx, lines.len);
                    lines_free(&lines);
                    return NULL;
                }
                if (pos > strlen(lines.data[idx])) {
                    *err = make_error("Append pos %zu out of bounds in line %zu (len %zu)",
                                      pos, idx, strlen(lines.data[idx]));
                    lines_free(&lines);
                    return NULL;
                }
                if (strchr(suffix, '\n') || strchr(suffix, '\r')) {
                    *err = make_error("Append suffix contains end-of-line");
                    lines_free(&lines);
                    return NULL;
                }
                char *new_line = str_insert_at(lines.data[idx], pos, suffix);
                free(lines.data[idx]);
                lines.data[idx] = new_line;
                break;
            }

            default: break;
        }
    }

    char *result = join_lines(&lines);
    lines_free(&lines);
    return result;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  diff::Hunk apply (in-place)
 *  Mirrors Hunk::apply() in src/diff.rs.
 *  Traverses the path then dispatches on HunkAction at the final node.
 * ═══════════════════════════════════════════════════════════════════════ */

static DocError hunk_apply(const DiffDoc__Hunk *h, GenericValue *root) {
    GenericValue *node = root;

    for (size_t i = 0; i < h->n_path; i++) {
        const DiffDoc__DocIndex *pi   = h->path[i];
        int                      last = (i == h->n_path - 1);

        /* ── DocIndex::Name ──────────────────────────────────────── */
        if (pi->kind_case == DIFF_DOC__DOC_INDEX__KIND_NAME) {
            const char *name = pi->name;
            if (node->kind != GV_MAP)
                return make_error("Path index not found: %s (not a map)", name);

            if (last) {
                const DiffDoc__HunkAction *act = h->value;
                switch (act->kind_case) {

                    case DIFF_DOC__HUNK_ACTION__KIND_REMOVE: {
                        GenericValue *old = map_remove(node, name);
                        gv_free(old);
                        break;
                    }

                    case DIFF_DOC__HUNK_ACTION__KIND_UPDATE:
                        map_set(node, name, gv_from_proto(act->update));
                        break;

                    case DIFF_DOC__HUNK_ACTION__KIND_INSERT:
                        map_set(node, name, gv_from_proto(act->insert));
                        break;

                    case DIFF_DOC__HUNK_ACTION__KIND_UPDATE_TXT: {
                        GenericValue **slot = map_get(node, name);
                        if (!slot || (*slot)->kind != GV_STRING)
                            return make_error("Expected string: %s", name);
                        DocError err = {NULL};
                        char *new_s = txt_apply(act->update_txt, (*slot)->string, &err);
                        if (is_err(&err)) return err;
                        free((*slot)->string);
                        (*slot)->string = new_s;
                        break;
                    }

                    /*
                     * Swap / Clone (map):
                     *   let a   = map[src_key]
                     *   let old = map.insert(name, clone(a))  // returns previous value
                     *   if Swap: map[src_key] = old  (or remove src_key if old was absent)
                     */
                    case DIFF_DOC__HUNK_ACTION__KIND_SWAP:
                    case DIFF_DOC__HUNK_ACTION__KIND_CLONE: {
                        const DiffDoc__DocIndex *src_di =
                            (act->kind_case == DIFF_DOC__HUNK_ACTION__KIND_SWAP)
                                ? act->swap : act->clone;
                        if (src_di->kind_case != DIFF_DOC__DOC_INDEX__KIND_NAME)
                            return make_error("index type must match: expected name");
                        const char *src_name = src_di->name;
                        GenericValue **src_slot = map_get(node, src_name);
                        if (!src_slot)
                            return make_error("Path not found: %s", src_name);
                        GenericValue *src_clone = gv_clone(*src_slot);
                        if (act->kind_case == DIFF_DOC__HUNK_ACTION__KIND_CLONE) {
                            map_set(node, name, src_clone);
                        } else {
                            /* Capture old value at 'name' before overwriting */
                            GenericValue *old_at_name = map_remove(node, name);
                            map_set(node, name, src_clone);
                            if (old_at_name)
                                map_set(node, src_name, old_at_name);
                            else {
                                GenericValue *rem = map_remove(node, src_name);
                                gv_free(rem);
                            }
                        }
                        break;
                    }

                    default: break;
                }
                return (DocError){NULL};
            } else {
                /* Traverse deeper */
                GenericValue **slot = map_get(node, name);
                if (!slot)
                    return make_error("Path not found: %s", name);
                node = *slot;
            }

        /* ── DocIndex::Idx ───────────────────────────────────────── */
        } else {
            uint64_t idx = pi->idx;
            if (node->kind != GV_ARRAY)
                return make_error("Path index not found: %llu (not an array)",
                                  (unsigned long long)idx);
            if (last) {
                const DiffDoc__HunkAction *act = h->value;
                switch (act->kind_case) {

                    case DIFF_DOC__HUNK_ACTION__KIND_REMOVE:
                        if (idx >= node->arr.len)
                            return make_error("Remove index %llu out of bounds %zu",
                                              (unsigned long long)idx, node->arr.len);
                        arr_remove(node, (size_t)idx);
                        break;

                    case DIFF_DOC__HUNK_ACTION__KIND_UPDATE:
                        if (idx >= node->arr.len)
                            return make_error("Update index %llu out of bounds",
                                              (unsigned long long)idx);
                        gv_free(node->arr.items[idx]);
                        node->arr.items[idx] = gv_from_proto(act->update);
                        break;

                    case DIFF_DOC__HUNK_ACTION__KIND_UPDATE_TXT:
                        if (idx >= node->arr.len)
                            return make_error("UpdateTxt index %llu out of bounds",
                                              (unsigned long long)idx);
                        if (node->arr.items[idx]->kind != GV_STRING)
                            return make_error("Expected string field: %llu",
                                              (unsigned long long)idx);
                        {
                            DocError err = {NULL};
                            char *new_s = txt_apply(act->update_txt,
                                                    node->arr.items[idx]->string,
                                                    &err);
                            if (is_err(&err)) return err;
                            free(node->arr.items[idx]->string);
                            node->arr.items[idx]->string = new_s;
                        }
                        break;

                    case DIFF_DOC__HUNK_ACTION__KIND_INSERT:
                        if (idx > node->arr.len)
                            return make_error("Insert index %llu out of bounds",
                                              (unsigned long long)idx);
                        arr_insert(node, (size_t)idx, gv_from_proto(act->insert));
                        break;

                    /*
                     * Swap (array):
                     *   mirrors fn swap() in src/diff.rs — simple vec.swap(a, b).
                     *   Out-of-bounds or equal indices → no-op.
                     */
                    case DIFF_DOC__HUNK_ACTION__KIND_SWAP: {
                        const DiffDoc__DocIndex *src_di = act->swap;
                        if (src_di->kind_case != DIFF_DOC__DOC_INDEX__KIND_IDX)
                            return make_error("index type must match: expected idx");
                        size_t a = (size_t)idx, b = (size_t)src_di->idx;
                        if (a < node->arr.len && b < node->arr.len && a != b) {
                            GenericValue *tmp = node->arr.items[a];
                            node->arr.items[a] = node->arr.items[b];
                            node->arr.items[b] = tmp;
                        }
                        break;
                    }

                    /*
                     * Clone (array):
                     *   mirrors fn copy() in src/diff.rs:
                     *   vec.insert(dst, clone(vec[src]))
                     *   dst > len or src >= len or dst == src → no-op.
                     */
                    case DIFF_DOC__HUNK_ACTION__KIND_CLONE: {
                        const DiffDoc__DocIndex *src_di = act->clone;
                        if (src_di->kind_case != DIFF_DOC__DOC_INDEX__KIND_IDX)
                            return make_error("index type must match: expected idx");
                        size_t dst = (size_t)idx, src = (size_t)src_di->idx;
                        if (dst <= node->arr.len && src < node->arr.len && dst != src)
                            arr_insert(node, dst, gv_clone(node->arr.items[src]));
                        break;
                    }

                    default: break;
                }
                return (DocError){NULL};
            } else {
                if (idx >= node->arr.len)
                    return make_error("Path not found: %llu", (unsigned long long)idx);
                node = node->arr.items[idx];
            }
        }
    }

    return (DocError){NULL};
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Top-level API
 * ═══════════════════════════════════════════════════════════════════════ */

/*
 * mismatches_apply_mut — in-place, mirrors MismatchDocMut::apply_mut.
 *
 * Doc variant:  iterates hunks; each failure is soft (collected) or hard
 *               (fail_fast), matching the Rust contract exactly.
 * Text variant: applies txt ops to a GV_STRING document.
 * Patch variant: raw GNU patch text cannot be applied without an external
 *               tool; an error is always returned for this variant.
 */
int mismatches_apply_mut(const DiffDoc__Mismatches *diff,
                         GenericValue              *doc,
                         int                        fail_fast,
                         DocError                  *errs,
                         size_t                     errs_cap,
                         size_t                    *errs_len_out)
{
    *errs_len_out = 0;

#define PUSH_ERR(e) \
    do { \
        if (*errs_len_out < errs_cap) errs[(*errs_len_out)++] = (e); \
        else doc_error_free(&(e)); \
    } while (0)

    switch (diff->kind_case) {

        case DIFF_DOC__MISMATCHES__KIND_PATCH: {
            /* GNU patch text — cannot apply directly in C without libpatch.
             * The raw text is in diff->patch; hand it to your patch tool. */
            DocError e = make_error(
                "Patch variant requires an external tool (e.g. GNU patch)");
            PUSH_ERR(e);
            if (fail_fast) return -1;
            break;
        }

        case DIFF_DOC__MISMATCHES__KIND_DOC: {
            /* Mirrors diff::MismatchDocMut<GenericValue>::apply_mut */
            const DiffDoc__DocMismatch *dm = diff->doc;
            for (size_t i = 0; dm && i < dm->n_hunks; i++) {
                DocError e = hunk_apply(dm->hunks[i], doc);
                if (is_err(&e)) {
                    PUSH_ERR(e);
                    if (fail_fast) return -1;
                }
            }
            break;
        }

        case DIFF_DOC__MISMATCHES__KIND_TEXT: {
            /* Mirrors txt::MismatchDocCow<String>::apply but in-place */
            if (doc->kind != GV_STRING) {
                DocError e = make_error(
                    "Text mismatch requires a GV_STRING document");
                PUSH_ERR(e);
                if (fail_fast) return -1;
                break;
            }
            DocError err = {NULL};
            char *new_s = txt_apply(diff->text, doc->string, &err);
            if (is_err(&err)) {
                PUSH_ERR(err);
                if (fail_fast) return -1;
            } else {
                free(doc->string);
                doc->string = new_s;
            }
            break;
        }

        default: break;
    }

#undef PUSH_ERR

    return (int)*errs_len_out;
}

/*
 * mismatches_apply — CoW, mirrors MismatchDocCow::apply.
 *
 * Clones `doc`, applies the diff in place with fail_fast=1.
 * Returns the new value on success (caller owns it), NULL on error.
 */
GenericValue *mismatches_apply(const DiffDoc__Mismatches *diff,
                               const GenericValue        *doc,
                               DocError                  *out_err)
{
    GenericValue *copy = gv_clone(doc);
    if (!copy) { *out_err = make_error("out of memory"); return NULL; }

    DocError errs[1];
    size_t   nerrs = 0;
    int rc = mismatches_apply_mut(diff, copy, /*fail_fast=*/1,
                                  errs, 1, &nerrs);
    if (rc < 0 && nerrs > 0) {
        *out_err = errs[0];
        gv_free(copy);
        return NULL;
    }
    out_err->msg = NULL;
    return copy;
}
