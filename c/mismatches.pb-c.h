/*
 * Hand-written stub for mismatches.pb-c.h
 *
 * Defines the same types that protoc-c would generate from
 * proto/mismatches.proto, but without the protobuf-c library dependency.
 * Only field access is needed — serialisation is never called from C.
 *
 * Replace this file with the real protoc-c output once protobuf-c is
 * installed:  protoc-c --c_out=. --proto_path=../proto mismatches.proto
 */
#ifndef MISMATCHES_PB_C_H
#define MISMATCHES_PB_C_H

#include <stddef.h>
#include <stdint.h>

/* ── forward declarations ─────────────────────────────────────────── */

typedef struct DiffDoc__GenericValue    DiffDoc__GenericValue;
typedef struct DiffDoc__GenericMap      DiffDoc__GenericMap;
typedef struct DiffDoc__GenericArray    DiffDoc__GenericArray;
typedef struct DiffDoc__TxtMismatch     DiffDoc__TxtMismatch;

/* ── DiffOp ───────────────────────────────────────────────────────── */

typedef enum {
    DIFF_DOC__DIFF_OP__KIND__NOT_SET = 0,
    DIFF_DOC__DIFF_OP__KIND_REMOVE   = 1,
    DIFF_DOC__DIFF_OP__KIND_INSERT   = 2,
    DIFF_DOC__DIFF_OP__KIND_UPDATE   = 3,
    DIFF_DOC__DIFF_OP__KIND_APPEND   = 4,
} DiffDoc__DiffOp__KindCase;

typedef struct { uint64_t index;                              } DiffDoc__DiffOpRemove;
typedef struct { uint64_t index; char *value;                 } DiffDoc__DiffOpInsert;
typedef struct { uint64_t index; char *value;                 } DiffDoc__DiffOpUpdate;
typedef struct { uint64_t index; uint64_t pos; char *value;   } DiffDoc__DiffOpAppend;

typedef struct {
    DiffDoc__DiffOp__KindCase kind_case;
    union {
        DiffDoc__DiffOpRemove *remove;
        DiffDoc__DiffOpInsert *insert;
        DiffDoc__DiffOpUpdate *update;
        DiffDoc__DiffOpAppend *append;
    };
} DiffDoc__DiffOp;

/* ── TxtMismatch ──────────────────────────────────────────────────── */

struct DiffDoc__TxtMismatch {
    size_t           n_ops;
    DiffDoc__DiffOp **ops;
};

/* ── GenericValue ─────────────────────────────────────────────────── */

typedef enum {
    DIFF_DOC__GENERIC_VALUE__KIND__NOT_SET = 0,
    DIFF_DOC__GENERIC_VALUE__KIND_NUMERIC  = 1,
    DIFF_DOC__GENERIC_VALUE__KIND_MAP      = 2,
    DIFF_DOC__GENERIC_VALUE__KIND_ARRAY    = 3,
    DIFF_DOC__GENERIC_VALUE__KIND_BOOLEAN  = 4,
    DIFF_DOC__GENERIC_VALUE__KIND_STRING   = 5,
    DIFF_DOC__GENERIC_VALUE__KIND_NULL     = 6,
} DiffDoc__GenericValue__KindCase;

typedef struct {
    char                  *key;
    DiffDoc__GenericValue *value;
} DiffDoc__GenericMap__FieldsEntry;

struct DiffDoc__GenericMap {
    size_t                           n_fields;
    DiffDoc__GenericMap__FieldsEntry **fields;
};

struct DiffDoc__GenericArray {
    size_t                 n_items;
    DiffDoc__GenericValue **items;
};

struct DiffDoc__GenericValue {
    DiffDoc__GenericValue__KindCase kind_case;
    union {
        char                  *numeric;  /* NUMERIC */
        DiffDoc__GenericMap   *map;      /* MAP     */
        DiffDoc__GenericArray *array;    /* ARRAY   */
        int                    boolean;  /* BOOLEAN */
        char                  *string;   /* STRING  */
        int                    null;     /* NULL    */
    };
};

/* ── DocIndex ─────────────────────────────────────────────────────── */

typedef enum {
    DIFF_DOC__DOC_INDEX__KIND__NOT_SET = 0,
    DIFF_DOC__DOC_INDEX__KIND_NAME     = 1,
    DIFF_DOC__DOC_INDEX__KIND_IDX      = 2,
} DiffDoc__DocIndex__KindCase;

typedef struct {
    DiffDoc__DocIndex__KindCase kind_case;
    union {
        char    *name;
        uint64_t idx;
    };
} DiffDoc__DocIndex;

/* ── HunkAction ───────────────────────────────────────────────────── */

typedef enum {
    DIFF_DOC__HUNK_ACTION__KIND__NOT_SET   = 0,
    DIFF_DOC__HUNK_ACTION__KIND_REMOVE     = 1,
    DIFF_DOC__HUNK_ACTION__KIND_UPDATE     = 2,
    DIFF_DOC__HUNK_ACTION__KIND_UPDATE_TXT = 3,
    DIFF_DOC__HUNK_ACTION__KIND_INSERT     = 4,
    DIFF_DOC__HUNK_ACTION__KIND_SWAP       = 5,
    DIFF_DOC__HUNK_ACTION__KIND_CLONE      = 6,
} DiffDoc__HunkAction__KindCase;

typedef struct {
    DiffDoc__HunkAction__KindCase kind_case;
    union {
        int                    remove;      /* bool — unit variant, set 1 */
        DiffDoc__GenericValue *update;
        DiffDoc__TxtMismatch  *update_txt;
        DiffDoc__GenericValue *insert;
        DiffDoc__DocIndex     *swap;
        DiffDoc__DocIndex     *clone;
    };
} DiffDoc__HunkAction;

/* ── Hunk ─────────────────────────────────────────────────────────── */

typedef struct {
    size_t            n_path;
    DiffDoc__DocIndex **path;
    DiffDoc__HunkAction *value;
} DiffDoc__Hunk;

/* ── DocMismatch ──────────────────────────────────────────────────── */

typedef struct {
    size_t         n_hunks;
    DiffDoc__Hunk **hunks;
} DiffDoc__DocMismatch;

/* ── Mismatches ───────────────────────────────────────────────────── */

typedef enum {
    DIFF_DOC__MISMATCHES__KIND__NOT_SET = 0,
    DIFF_DOC__MISMATCHES__KIND_PATCH    = 1,
    DIFF_DOC__MISMATCHES__KIND_DOC      = 2,
    DIFF_DOC__MISMATCHES__KIND_TEXT     = 3,
} DiffDoc__Mismatches__KindCase;

typedef struct {
    DiffDoc__Mismatches__KindCase kind_case;
    union {
        char                 *patch;
        DiffDoc__DocMismatch *doc;
        DiffDoc__TxtMismatch *text;
    };
} DiffDoc__Mismatches;

#endif /* MISMATCHES_PB_C_H */
