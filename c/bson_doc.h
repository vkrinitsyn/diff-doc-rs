/*%LPH%*/

#ifndef DIFF_DOC_BSON_H
#define DIFF_DOC_BSON_H

/*
 * BSON, as DocumentDB and MongoDB store a document, as a GenericValue, and back -
 * optional: build with `make BSON=1`, or compile bson_doc.c beside diff_apply.c.
 *
 * The mapping is src/bson_doc.rs's, so a patch the Rust side makes of two BSON
 * documents applies here to the same tree:
 *   - JSON's types are themselves: a string, a bool, null, a document, an array, an
 *     int32 (a number), a double (a number with a fraction or an exponent: "1.0")
 *   - an int64: a number out of int32's range, else {"$numberLong": "7"}
 *   - the rest as relaxed extended JSON: {"$oid": ...}, {"$date": "...Z"},
 *     {"$numberDecimal": ...}, {"$binary": {"base64": ..., "subType": ...}}, ...
 *
 * gv_to_bson writes a document's keys sorted, the root's "_id" first - as the Rust
 * side does, so each node writes the same bytes. Self-contained: no libbson.
 * Numbers are read with strtod: LC_NUMERIC must be "C", as PostgreSQL keeps it.
 */

#include "diff_apply.h"

/*
 * The document of `len` bytes at `data` (bytes past its own length are not read).
 * On error returns NULL and writes a message into *out_err ("BSON: ...").
 * Caller must gv_free() the result.
 */
GenericValue *gv_from_bson(const uint8_t *data, size_t len, DocError *out_err);

/*
 * The BSON of `gv`, a map: *out (malloc'ed, caller frees) of *out_len bytes.
 * Returns 0, or -1 and a message in *out_err: DD-E501 not a document, DD-E502 a
 * number past int64, "BSON: ..." a value of extended JSON that does not read.
 */
int gv_to_bson(const GenericValue *gv, uint8_t **out, size_t *out_len, DocError *out_err);

#endif /* DIFF_DOC_BSON_H */
