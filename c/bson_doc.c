/*%LPH%*/

/*
 * BSON <-> GenericValue: the mapping of src/bson_doc.rs, see bson_doc.h.
 * Self-contained C11: no libbson, no 128-bit integers (MSVC has none).
 */

#include <errno.h>
#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "bson_doc.h"

/* deeper than this, a document is refused: the reader recurses */
#define MAX_DEPTH 200

/* the last millisecond of 9999-12-31: a later date has no RFC 3339 of 4 digits */
#define MAX_RFC3339 INT64_C(253402300799999)

static const char DIGITS[] = "0123456789";

/* ═══════════════════════════════════════════════════════════════════════
 *  Errors
 * ═══════════════════════════════════════════════════════════════════════ */

#if defined(__GNUC__)
static void fail(DocError *e, const char *fmt, ...) __attribute__((format(printf, 2, 3)));
#endif

/* the first error only: the one that stopped the work */
static void fail(DocError *e, const char *fmt, ...) {
    if (!e || e->msg) return;
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(buf, sizeof buf, fmt, ap);
    va_end(ap);
    size_t n = strlen(buf) + 1;
    e->msg = malloc(n);
    if (e->msg) memcpy(e->msg, buf, n);
}

static char *dup_n(const char *s, size_t n) {
    char *d = malloc(n + 1);
    if (d) { memcpy(d, s, n); d[n] = 0; }
    return d;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Numbers as the Rust side reads and writes them
 * ═══════════════════════════════════════════════════════════════════════ */

/* [+-]digits */
static int int_syntax(const char *s) {
    if (*s == '+' || *s == '-') s++;
    size_t n = strspn(s, DIGITS);
    return n > 0 && s[n] == 0;
}

/* Rust's f64 grammar, its words aside: [+-] digits [. digits] [e [+-] digits] */
static int f64_syntax(const char *s) {
    if (*s == '+' || *s == '-') s++;
    size_t a = strspn(s, DIGITS), b = 0;
    s += a;
    if (*s == '.') { s++; b = strspn(s, DIGITS); s += b; }
    if (a + b == 0) return 0;
    if (*s == 'e' || *s == 'E') {
        s++;
        if (*s == '+' || *s == '-') s++;
        size_t c = strspn(s, DIGITS);
        if (c == 0) return 0;
        s += c;
    }
    return *s == 0;
}

static int eq_nocase(const char *a, const char *b) {
    for (; *a && *b; a++, b++) {
        char x = *a >= 'A' && *a <= 'Z' ? *a + 32 : *a;
        if (x != *b) return 0;
    }
    return *a == *b;
}

/* Rust's str::parse::<f64>: its words too - inf, infinity, nan, any case */
static int parse_f64(const char *s, double *out) {
    const char *w = (*s == '+' || *s == '-') ? s + 1 : s;
    if (eq_nocase(w, "inf") || eq_nocase(w, "infinity")) { *out = *s == '-' ? -HUGE_VAL : HUGE_VAL; return 1; }
    if (eq_nocase(w, "nan")) { *out = *s == '-' ? -NAN : NAN; return 1; }
    if (!f64_syntax(s)) return 0;
    *out = strtod(s, NULL);
    return 1;
}

/* generic.rs's is_numeric: an integer or a finite float */
static int is_numeric(const char *s) {
    double d;
    return f64_syntax(s) && parse_f64(s, &d) && isfinite(d);
}

static int parse_i64(const char *s, int64_t *out) {
    if (!int_syntax(s)) return 0;
    errno = 0;
    long long v = strtoll(s, NULL, 10);
    if (errno == ERANGE) return 0;
    *out = v;
    return 1;
}

static int parse_u32(const char *s, uint32_t *out) {
    int64_t v;
    if (*s == '-' || !parse_i64(s, &v) || v > UINT32_MAX) return 0;
    *out = (uint32_t)v;
    return 1;
}

/*
 * A double as the Rust side writes it (Debug, through serde_json): its shortest
 * digits, with a fraction ("1.0") from 1e-4 to 1e16, else an exponent ("1e+16",
 * "1.5e-7"). `out` holds 64 bytes.
 */
static void fmt_double(double x, char *out) {
    char buf[40];
    for (int p = 1; p <= 17; p++) {
        snprintf(buf, sizeof buf, "%.*e", p - 1, x);
        if (strtod(buf, NULL) == x) break;
    }
    /* buf: [-]d[.ddd]e[+-]xx */
    const char *s = buf;
    char *o = out;
    if (*s == '-') *o++ = *s++;
    char digits[24];
    size_t n = 0;
    for (; *s != 'e'; s++)
        if (*s != '.') digits[n++] = *s;
    while (n > 1 && digits[n - 1] == '0') n--;
    int exp = atoi(s + 1);
    double ax = fabs(x);
    if (ax == 0 || (ax >= 1e-4 && ax < 1e16)) {
        int point = exp + 1;   /* the digits before the point */
        if (point <= 0) {
            o += sprintf(o, "0.");
            for (int i = 0; i < -point; i++) *o++ = '0';
            memcpy(o, digits, n); o += n;
        } else if ((size_t)point >= n) {
            memcpy(o, digits, n); o += n;
            for (size_t i = n; i < (size_t)point; i++) *o++ = '0';
            o += sprintf(o, ".0");
        } else {
            memcpy(o, digits, point); o += point;
            *o++ = '.';
            memcpy(o, digits + point, n - point); o += n - point;
        }
        *o = 0;
    } else {
        *o++ = digits[0];
        if (n > 1) { *o++ = '.'; memcpy(o, digits + 1, n - 1); o += n - 1; }
        sprintf(o, "e%s%d", exp > 0 ? "+" : "", exp);
    }
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Hex, base64, UTF-8
 * ═══════════════════════════════════════════════════════════════════════ */

static int hex_val(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

static void hex_of(const uint8_t *b, size_t n, char *out) {
    static const char H[] = "0123456789abcdef";
    for (size_t i = 0; i < n; i++) { out[2 * i] = H[b[i] >> 4]; out[2 * i + 1] = H[b[i] & 15]; }
    out[2 * n] = 0;
}

/* exactly 2n hex digits */
static int hex_to(const char *s, uint8_t *out, size_t n) {
    if (strlen(s) != 2 * n) return 0;
    for (size_t i = 0; i < n; i++) {
        int h = hex_val(s[2 * i]), l = hex_val(s[2 * i + 1]);
        if (h < 0 || l < 0) return 0;
        out[i] = (uint8_t)(h << 4 | l);
    }
    return 1;
}

static const char B64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static char *base64_of(const uint8_t *b, size_t n) {
    char *out = malloc(4 * ((n + 2) / 3) + 1), *o = out;
    if (!out) return NULL;
    for (size_t i = 0; i < n; i += 3) {
        uint32_t v = (uint32_t)b[i] << 16 | (i + 1 < n ? (uint32_t)b[i + 1] << 8 : 0) | (i + 2 < n ? b[i + 2] : 0);
        *o++ = B64[v >> 18 & 63];
        *o++ = B64[v >> 12 & 63];
        *o++ = i + 1 < n ? B64[v >> 6 & 63] : '=';
        *o++ = i + 2 < n ? B64[v & 63] : '=';
    }
    *o = 0;
    return out;
}

/* padded, as the bson crate takes it; *out malloc'ed */
static int base64_to(const char *s, uint8_t **out, size_t *len) {
    size_t n = strlen(s);
    if (n % 4) return 0;
    uint8_t *b = malloc(n / 4 * 3 + 1);
    if (!b) return 0;
    size_t k = 0;
    for (size_t i = 0; i < n; i += 4) {
        uint32_t v = 0;
        int pad = 0;
        for (int j = 0; j < 4; j++) {
            char c = s[i + j];
            const char *p = c ? strchr(B64, c) : NULL;
            if (c == '=' && i + 4 == n && j >= 2 && (j == 3 || s[i + 3] == '=')) { pad++; v <<= 6; continue; }
            if (!p || pad) { free(b); return 0; }
            v = v << 6 | (uint32_t)(p - B64);
        }
        b[k++] = v >> 16 & 255;
        if (pad < 2) b[k++] = v >> 8 & 255;
        if (pad < 1) b[k++] = v & 255;
    }
    *out = b;
    *len = k;
    return 1;
}

static int utf8_ok(const uint8_t *s, size_t n) {
    for (size_t i = 0; i < n;) {
        uint8_t c = s[i];
        size_t k;
        uint32_t cp;
        if (c < 0x80) { i++; continue; }
        else if ((c & 0xE0) == 0xC0) { k = 1; cp = c & 0x1F; }
        else if ((c & 0xF0) == 0xE0) { k = 2; cp = c & 0x0F; }
        else if ((c & 0xF8) == 0xF0) { k = 3; cp = c & 0x07; }
        else return 0;
        if (i + k >= n) return 0;
        for (size_t j = 1; j <= k; j++) {
            if ((s[i + j] & 0xC0) != 0x80) return 0;
            cp = cp << 6 | (s[i + j] & 0x3F);
        }
        if ((k == 1 && cp < 0x80) || (k == 2 && cp < 0x800) || (k == 3 && cp < 0x10000)
            || cp > 0x10FFFF || (cp >= 0xD800 && cp <= 0xDFFF)) return 0;
        i += k + 1;
    }
    return 1;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Decimal128 as text, and back: the bson crate's (the BSON spec's) forms
 * ═══════════════════════════════════════════════════════════════════════ */

/* 128 bits as 4 words, the most significant first */
typedef struct { uint32_t w[4]; } U128;

static U128 u128_of(uint64_t hi, uint64_t lo) {
    U128 v = {{ (uint32_t)(hi >> 32), (uint32_t)hi, (uint32_t)(lo >> 32), (uint32_t)lo }};
    return v;
}

static int u128_zero(const U128 *v) { return !(v->w[0] | v->w[1] | v->w[2] | v->w[3]); }

static int u128_gt(const U128 *a, const U128 *b) {
    for (int i = 0; i < 4; i++)
        if (a->w[i] != b->w[i]) return a->w[i] > b->w[i];
    return 0;
}

static unsigned u128_div10(U128 *v) {
    uint64_t r = 0;
    for (int i = 0; i < 4; i++) {
        uint64_t cur = r << 32 | v->w[i];
        v->w[i] = (uint32_t)(cur / 10);
        r = cur % 10;
    }
    return (unsigned)r;
}

static void u128_mul10_add(U128 *v, unsigned d) {
    uint64_t carry = d;
    for (int i = 3; i >= 0; i--) {
        uint64_t cur = (uint64_t)v->w[i] * 10 + carry;
        v->w[i] = (uint32_t)cur;
        carry = cur >> 32;
    }
}

static uint64_t le64(const uint8_t *p) {
    uint64_t v = 0;
    for (int i = 7; i >= 0; i--) v = v << 8 | p[i];
    return v;
}

static void put_le64(uint8_t *p, uint64_t v) {
    for (int i = 0; i < 8; i++) { p[i] = (uint8_t)v; v >>= 8; }
}

/* 34 nines: the largest coefficient */
#define DEC_MAX_HI UINT64_C(0x0001ed09bead87c0)
#define DEC_MAX_LO UINT64_C(0x378d8e63ffffffff)
#define DEC_BIAS 6176
#define DEC_TINY (-6176)
#define DEC_EMAX 6111

/* `out` holds 64 bytes */
static void dec_to_str(const uint8_t *b, char *out) {
    uint64_t lo = le64(b), hi = le64(b + 8);
    int sign = (int)(hi >> 63);
    if ((hi >> 59 & 0xF) == 0xF) {
        /* no sign for NaN, as MongoDB has it */
        strcpy(out, hi >> 58 & 1 ? "NaN" : sign ? "-Infinity" : "Infinity");
        return;
    }
    int exp;
    U128 c;
    if ((hi >> 61 & 3) == 3) {
        /* a coefficient past 2^113: past the largest, read as zero */
        exp = (int)(hi >> 47 & 0x3FFF) - DEC_BIAS;
        c = u128_of(0, 0);
    } else {
        U128 max = u128_of(DEC_MAX_HI, DEC_MAX_LO);
        exp = (int)(hi >> 49 & 0x3FFF) - DEC_BIAS;
        c = u128_of(hi & ((UINT64_C(1) << 49) - 1), lo);
        if (u128_gt(&c, &max)) c = u128_of(0, 0);
    }
    char rev[40], d[40];
    int n = 0;
    do { rev[n++] = (char)('0' + u128_div10(&c)); } while (!u128_zero(&c));
    for (int i = 0; i < n; i++) d[i] = rev[n - 1 - i];
    d[n] = 0;
    char *o = out;
    if (sign) *o++ = '-';
    int adj = exp + n - 1;
    if (exp <= 0 && adj >= -6) {
        if (exp == 0) {
            strcpy(o, d);
        } else if (-exp >= n) {
            o += sprintf(o, "0.");
            for (int i = 0; i < -exp - n; i++) *o++ = '0';
            strcpy(o, d);
        } else {
            memcpy(o, d, n + exp); o += n + exp;
            *o++ = '.';
            strcpy(o, d + n + exp);
        }
    } else {
        *o++ = d[0];
        if (n > 1) { *o++ = '.'; memcpy(o, d + 1, n - 1); o += n - 1; }
        sprintf(o, "E%s%d", adj > 0 ? "+" : "", adj);
    }
}

/* the bson crate's FromStr: rounds only off zeros, else fails */
static int dec_from_str(const char *s, uint8_t *out) {
    int sign = 0;
    if (*s == '-' || *s == '+') { sign = *s == '-'; s++; }
    uint64_t hi = (uint64_t)sign << 63, lo = 0;
    if (eq_nocase(s, "nan") || eq_nocase(s, "snan") || eq_nocase(s, "infinity") || eq_nocase(s, "inf")) {
        if (eq_nocase(s, "nan")) hi |= UINT64_C(0x1F) << 58;
        else if (eq_nocase(s, "snan")) hi |= UINT64_C(0x1F) << 58 | UINT64_C(1) << 57;
        else hi |= UINT64_C(0x1E) << 58;
        put_le64(out, lo);
        put_le64(out + 8, hi);
        return 1;
    }
    size_t len = strlen(s);
    char *t = malloc(len + 1);
    if (!t) return 0;
    for (size_t i = 0; i <= len; i++) t[i] = s[i] >= 'A' && s[i] <= 'Z' ? s[i] + 32 : s[i];
    int ok = 0;
    long long exp = 0;
    char *e = strchr(t, 'e');
    if (e) {
        *e = 0;
        /* an exponent of more digits is past any decimal's */
        if (!int_syntax(e + 1) || strlen(e + 1) > 18) goto out;
        exp = strtoll(e + 1, NULL, 10);
    }
    /* the digits, without the point: t is rewritten in place */
    char *dot = strchr(t, '.');
    if (dot) {
        size_t post = strlen(dot + 1);
        exp -= (long long)post;
        memmove(dot, dot + 1, post + 1);
    }
    char *d = t;
    if (strspn(d, DIGITS) != strlen(d)) goto out;
    while (*d == '0') d++;
    if (!*d) d = "0";
    size_t n = strlen(d);
    int zero = strcmp(d, "0") == 0;
    if (n > 34) {
        if (strspn(d + 34, "0") != n - 34) goto out;
        exp += (long long)(n - 34);
        n = 34;
    }
    if (exp < DEC_TINY) {
        if (!zero) {
            long long delta = DEC_TINY - exp;
            if (delta > (long long)n) goto out;
            size_t keep = n - (size_t)delta;
            if (strspn(d + keep, "0") < (size_t)delta) goto out;
            n = keep;
        }
        exp = DEC_TINY;
    }
    char padded[40];
    if (exp > DEC_EMAX) {
        if (!zero) {
            long long delta = exp - DEC_EMAX;
            if ((long long)n + delta > 34) goto out;
            memcpy(padded, d, n);
            memset(padded + n, '0', (size_t)delta);
            n += (size_t)delta;
            d = padded;
        }
        exp = DEC_EMAX;
    }
    if (n == 0) goto out;
    U128 c = u128_of(0, 0);
    for (size_t i = 0; i < n; i++) u128_mul10_add(&c, (unsigned)(d[i] - '0'));
    hi |= (uint64_t)(exp + DEC_BIAS) << 49 | ((uint64_t)c.w[0] << 32 | c.w[1]);
    lo = (uint64_t)c.w[2] << 32 | c.w[3];
    put_le64(out, lo);
    put_le64(out + 8, hi);
    ok = 1;
out:
    free(t);
    return ok;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  Dates: RFC 3339 in UTC, and back
 * ═══════════════════════════════════════════════════════════════════════ */

/* days since 1970-01-01 of a civil date (H. Hinnant's algorithm) */
static int64_t days_of(int64_t y, int m, int d) {
    y -= m <= 2;
    int64_t era = (y >= 0 ? y : y - 399) / 400;
    int64_t yoe = y - era * 400;
    int64_t doy = (153 * (m + (m > 2 ? -3 : 9)) + 2) / 5 + d - 1;
    int64_t doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    return era * 146097 + doe - 719468;
}

static void civil_of(int64_t z, int64_t *y, int *m, int *d) {
    z += 719468;
    int64_t era = (z >= 0 ? z : z - 146096) / 146097;
    int64_t doe = z - era * 146097;
    int64_t yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    int64_t doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    int64_t mp = (5 * doy + 2) / 153;
    *d = (int)(doy - (153 * mp + 2) / 5 + 1);
    *m = (int)(mp < 10 ? mp + 3 : mp - 9);
    *y = yoe + era * 400 + (*m <= 2);
}

/* 0 <= ms <= MAX_RFC3339: "2024-05-01T10:00:00.12Z", the fraction without its zeros */
static void fmt_date(int64_t ms, char *out) {
    int64_t days = ms / 86400000, rem = ms % 86400000, y;
    int m, d;
    civil_of(days, &y, &m, &d);
    int n = sprintf(out, "%04d-%02d-%02dT%02d:%02d:%02d", (int)y, m, d,
                    (int)(rem / 3600000), (int)(rem / 60000 % 60), (int)(rem / 1000 % 60));
    int frac = (int)(rem % 1000);
    if (frac) {
        n += sprintf(out + n, ".%03d", frac);
        while (out[n - 1] == '0') n--;
    }
    strcpy(out + n, "Z");
}

static int num2(const char *s, int *v) {
    if (s[0] < '0' || s[0] > '9' || s[1] < '0' || s[1] > '9') return 0;
    *v = (s[0] - '0') * 10 + s[1] - '0';
    return 1;
}

/* RFC 3339, to milliseconds as the bson crate truncates them: toward zero */
static int parse_date(const char *s, int64_t *out) {
    int c, yy, mo, d, h, mi, se, oh = 0, om = 0;
    if (strlen(s) < 20 || !num2(s, &c) || !num2(s + 2, &yy) || s[4] != '-' || !num2(s + 5, &mo) || s[7] != '-'
        || !num2(s + 8, &d) || (s[10] != 'T' && s[10] != 't') || !num2(s + 11, &h) || s[13] != ':'
        || !num2(s + 14, &mi) || s[16] != ':' || !num2(s + 17, &se))
        return 0;
    int y = c * 100 + yy;
    static const int MDAYS[] = {31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};
    int leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    if (mo < 1 || mo > 12 || d < 1 || d > MDAYS[mo - 1] || (mo == 2 && d == 29 && !leap)
        || h > 23 || mi > 59 || se > 59)
        return 0;
    const char *p = s + 19;
    int64_t ns = 0;
    if (*p == '.') {
        p++;
        size_t k = strspn(p, DIGITS);
        if (k == 0) return 0;
        for (size_t i = 0; i < 9; i++) ns = ns * 10 + (i < k ? p[i] - '0' : 0);
        p += k;
    }
    int sign = 0;
    if (*p == 'Z' || *p == 'z') {
        p++;
    } else if (*p == '+' || *p == '-') {
        sign = *p == '-' ? -1 : 1;
        if (!num2(p + 1, &oh) || p[3] != ':' || !num2(p + 4, &om) || oh > 23 || om > 59) return 0;
        p += 6;
    } else {
        return 0;
    }
    if (*p) return 0;
    int64_t secs = days_of(y, mo, d) * 86400 + h * 3600 + mi * 60 + se - sign * (oh * 3600 + om * 60);
    int64_t ms = secs * 1000 + ns / 1000000;
    if (ms < 0 && ns % 1000000) ms++;
    *out = ms;
    return 1;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  GenericValue construction
 * ═══════════════════════════════════════════════════════════════════════ */

static GenericValue *gv_new(GvKind k) {
    GenericValue *g = calloc(1, sizeof *g);
    if (g) g->kind = k;
    return g;
}

static GenericValue *gv_text(GvKind k, const char *s) {
    char *c = dup_n(s, strlen(s));
    GenericValue *g = c ? gv_new(k) : NULL;
    if (!g) { free(c); return NULL; }
    g->string = c;
    return g;
}

/* a string of extended JSON, as from_json reads it: a number's a Numeric */
static GenericValue *gv_js(const char *s) {
    return gv_text(is_numeric(s) ? GV_NUMERIC : GV_STRING, s);
}

/* takes `v`; a key again replaces its value, as the bson crate reads it */
static int map_put(GenericValue *m, const char *key, size_t klen, GenericValue *v) {
    if (!v) return 0;
    for (size_t i = 0; i < m->map.len; i++) {
        if (strlen(m->map.ents[i].key) == klen && memcmp(m->map.ents[i].key, key, klen) == 0) {
            gv_free(m->map.ents[i].val);
            m->map.ents[i].val = v;
            return 1;
        }
    }
    if (m->map.len == m->map.cap) {
        size_t cap = m->map.cap ? m->map.cap * 2 : 4;
        MapEntry *e = realloc(m->map.ents, cap * sizeof *e);
        if (!e) { gv_free(v); return 0; }
        m->map.ents = e;
        m->map.cap = cap;
    }
    char *k = dup_n(key, klen);
    if (!k) { gv_free(v); return 0; }
    m->map.ents[m->map.len].key = k;
    m->map.ents[m->map.len].val = v;
    m->map.len++;
    return 1;
}

static int arr_push(GenericValue *a, GenericValue *v) {
    if (!v) return 0;
    if (a->arr.len == a->arr.cap) {
        size_t cap = a->arr.cap ? a->arr.cap * 2 : 4;
        GenericValue **items = realloc(a->arr.items, cap * sizeof *items);
        if (!items) { gv_free(v); return 0; }
        a->arr.items = items;
        a->arr.cap = cap;
    }
    a->arr.items[a->arr.len++] = v;
    return 1;
}

/* {k1: v1[, k2: v2]}: takes the values, NULL if one is */
static GenericValue *map_of(const char *k1, GenericValue *v1, const char *k2, GenericValue *v2) {
    GenericValue *m = gv_new(GV_MAP);
    if (!m || !v1 || (k2 && !v2)) { gv_free(m); gv_free(v1); gv_free(v2); return NULL; }
    /* map_put frees its value when it fails */
    if (!map_put(m, k1, strlen(k1), v1)) { gv_free(m); gv_free(v2); return NULL; }
    if (k2 && !map_put(m, k2, strlen(k2), v2)) { gv_free(m); return NULL; }
    return m;
}

/* ═══════════════════════════════════════════════════════════════════════
 *  BSON → GenericValue
 * ═══════════════════════════════════════════════════════════════════════ */

static int32_t le32(const uint8_t *p) {
    return (int32_t)((uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24);
}

/* BSON's string: int32 length, its bytes, NUL; *s malloc'ed */
static int read_str(const uint8_t *p, size_t avail, char **s, size_t *used, DocError *err) {
    if (avail < 5) { fail(err, "BSON: a string past the end of its document"); return 0; }
    int32_t n = le32(p);
    if (n < 1 || (size_t)n > avail - 4 || p[4 + n - 1] != 0) { fail(err, "BSON: a string of a wrong length"); return 0; }
    if (memchr(p + 4, 0, (size_t)n - 1)) { fail(err, "BSON: a string with a NUL byte"); return 0; }
    if (!utf8_ok(p + 4, (size_t)n - 1)) { fail(err, "BSON: a string not of UTF-8"); return 0; }
    *s = dup_n((const char *)p + 4, (size_t)n - 1);
    *used = 4 + (size_t)n;
    if (!*s) fail(err, "BSON: out of memory");
    return *s != NULL;
}

static GenericValue *read_doc(const uint8_t *p, size_t avail, int array, int ext, int depth,
                              size_t *used, DocError *err);

/* the value of type `t` at p; `ext`: inside $scope, relaxed extended JSON throughout */
static GenericValue *read_value(uint8_t t, const uint8_t *p, size_t avail, int ext, int depth,
                                size_t *used, DocError *err) {
    char buf[64], *s = NULL, *s2 = NULL;
    size_t n = 0, n2 = 0;
    GenericValue *v = NULL;
    static const size_t FIXED[] = { [0x01] = 8, [0x07] = 12, [0x08] = 1, [0x09] = 8, [0x10] = 4,
                                    [0x11] = 8, [0x12] = 8, [0x13] = 16 };
    if (t < sizeof FIXED / sizeof *FIXED && FIXED[t] > avail) {
        fail(err, "BSON: a value past the end of its document");
        return NULL;
    }
    switch (t) {
    case 0x01: {   /* double */
        uint64_t bits = le64(p);
        double d;
        memcpy(&d, &bits, sizeof d);
        *used = 8;
        if (isfinite(d)) { fmt_double(d, buf); return gv_text(GV_NUMERIC, buf); }
        return map_of("$numberDouble", gv_js(isinf(d) ? (d < 0 ? "-Infinity" : "Infinity")
                                                       : signbit(d) ? "-NaN" : "NaN"), NULL, NULL);
    }
    case 0x02:     /* string */
    case 0x0D:     /* code */
    case 0x0E:     /* symbol */
        if (!read_str(p, avail, &s, used, err)) return NULL;
        v = t == 0x02 ? (ext ? gv_js(s) : gv_text(GV_STRING, s))
                      : map_of(t == 0x0D ? "$code" : "$symbol", gv_js(s), NULL, NULL);
        free(s);
        break;
    case 0x03:
    case 0x04:
        return read_doc(p, avail, t == 0x04, ext, depth + 1, used, err);
    case 0x05: {   /* binary */
        if (avail < 5) { fail(err, "BSON: a binary past the end of its document"); return NULL; }
        int32_t len = le32(p);
        uint8_t sub = p[4];
        if (len < 0 || (size_t)len > avail - 5) { fail(err, "BSON: a binary of a wrong length"); return NULL; }
        const uint8_t *b = p + 5;
        size_t bl = (size_t)len;
        if (sub == 0x02) {   /* the old binary: its length once more */
            if (bl < 4 || le32(b) != len - 4) { fail(err, "BSON: an old binary of a wrong length"); return NULL; }
            b += 4; bl -= 4;
        }
        *used = 5 + (size_t)len;
        char *b64 = base64_of(b, bl);
        hex_of(&sub, 1, buf);
        v = b64 ? map_of("$binary", map_of("base64", gv_js(b64), "subType", gv_js(buf)), NULL, NULL) : NULL;
        free(b64);
        break;
    }
    case 0x06:
        *used = 0;
        v = gv_new(GV_BOOL);
        if (v) v->boolean = 1;
        return map_of("$undefined", v, NULL, NULL);
    case 0x07:
        hex_of(p, 12, buf);
        *used = 12;
        return map_of("$oid", gv_js(buf), NULL, NULL);
    case 0x08:
        if (p[0] > 1) { fail(err, "BSON: a boolean neither 0 nor 1"); return NULL; }
        *used = 1;
        v = gv_new(GV_BOOL);
        if (v) v->boolean = p[0];
        break;
    case 0x09: {   /* date */
        int64_t ms = (int64_t)le64(p);
        *used = 8;
        if (ms >= 0 && ms <= MAX_RFC3339) {
            fmt_date(ms, buf);
            return map_of("$date", gv_js(buf), NULL, NULL);
        }
        snprintf(buf, sizeof buf, "%lld", (long long)ms);
        return map_of("$date", map_of("$numberLong", gv_js(buf), NULL, NULL), NULL, NULL);
    }
    case 0x0A:
        *used = 0;
        v = gv_new(GV_NULL);
        break;
    case 0x0B: {   /* regex: two cstrings, the options sorted */
        const uint8_t *z1 = memchr(p, 0, avail);
        const uint8_t *z2 = z1 ? memchr(z1 + 1, 0, avail - (size_t)(z1 + 1 - p)) : NULL;
        if (!z2) { fail(err, "BSON: a regex past the end of its document"); return NULL; }
        n = (size_t)(z1 - p);
        n2 = (size_t)(z2 - z1 - 1);
        if (!utf8_ok(p, n) || !utf8_ok(z1 + 1, n2)) { fail(err, "BSON: a regex not of UTF-8"); return NULL; }
        *used = n + n2 + 2;
        s = dup_n((const char *)p, n);
        s2 = dup_n((const char *)z1 + 1, n2);
        if (s2)
            for (size_t i = 1; i < n2; i++)
                for (size_t j = i; j > 0 && (uint8_t)s2[j - 1] > (uint8_t)s2[j]; j--) {
                    char c = s2[j]; s2[j] = s2[j - 1]; s2[j - 1] = c;
                }
        v = s && s2 ? map_of("$regularExpression", map_of("pattern", gv_js(s), "options", gv_js(s2)), NULL, NULL) : NULL;
        free(s);
        free(s2);
        break;
    }
    case 0x0C: {   /* DBPointer: a string, an ObjectId */
        if (!read_str(p, avail, &s, &n, err)) return NULL;
        if (avail - n < 12) { free(s); fail(err, "BSON: a DBPointer past the end of its document"); return NULL; }
        hex_of(p + n, 12, buf);
        *used = n + 12;
        v = map_of("$dbPointer", map_of("$ref", gv_js(s), "$id", map_of("$oid", gv_js(buf), NULL, NULL)), NULL, NULL);
        free(s);
        break;
    }
    case 0x0F: {   /* code with scope: int32 length, a string, a document */
        if (avail < 4) { fail(err, "BSON: a code with scope past the end of its document"); return NULL; }
        int32_t len = le32(p);
        if (len < 4 || (size_t)len > avail) { fail(err, "BSON: a code with scope of a wrong length"); return NULL; }
        if (!read_str(p + 4, (size_t)len - 4, &s, &n, err)) return NULL;
        GenericValue *scope = read_doc(p + 4 + n, (size_t)len - 4 - n, 0, 1, depth + 1, &n2, err);
        if (scope && 4 + n + n2 != (size_t)len) {
            gv_free(scope);
            scope = NULL;
            fail(err, "BSON: a code with scope of a wrong length");
        }
        *used = (size_t)len;
        v = scope ? map_of("$code", gv_js(s), "$scope", scope) : NULL;
        free(s);
        break;
    }
    case 0x10:
        snprintf(buf, sizeof buf, "%ld", (long)le32(p));
        *used = 4;
        return gv_text(GV_NUMERIC, buf);
    case 0x11: {   /* timestamp: the increment, then the time */
        char t2[16];
        snprintf(buf, sizeof buf, "%lu", (unsigned long)(uint32_t)(le64(p) >> 32));
        snprintf(t2, sizeof t2, "%lu", (unsigned long)(uint32_t)le64(p));
        *used = 8;
        return map_of("$timestamp", map_of("t", gv_text(GV_NUMERIC, buf), "i", gv_text(GV_NUMERIC, t2)), NULL, NULL);
    }
    case 0x12: {
        int64_t i = (int64_t)le64(p);
        snprintf(buf, sizeof buf, "%lld", (long long)i);
        *used = 8;
        if (ext || i < INT32_MIN || i > INT32_MAX) return gv_text(GV_NUMERIC, buf);
        return map_of("$numberLong", gv_js(buf), NULL, NULL);
    }
    case 0x13:
        dec_to_str(p, buf);
        *used = 16;
        return map_of("$numberDecimal", gv_js(buf), NULL, NULL);
    case 0xFF:
    case 0x7F:
        *used = 0;
        return map_of(t == 0xFF ? "$minKey" : "$maxKey", gv_text(GV_NUMERIC, "1"), NULL, NULL);
    default:
        fail(err, "BSON: an element of unknown type 0x%02x", t);
        return NULL;
    }
    return v;
}

static GenericValue *read_doc(const uint8_t *p, size_t avail, int array, int ext, int depth,
                              size_t *used, DocError *err) {
    if (depth > MAX_DEPTH) { fail(err, "BSON: a document nested deeper than %d", MAX_DEPTH); return NULL; }
    if (avail < 5) { fail(err, "BSON: a document past the end of its bytes"); return NULL; }
    int32_t len = le32(p);
    if (len < 5 || (size_t)len > avail || p[len - 1] != 0) {
        fail(err, "BSON: a document of a wrong length");
        return NULL;
    }
    GenericValue *g = gv_new(array ? GV_ARRAY : GV_MAP);
    if (!g) { fail(err, "BSON: out of memory"); return NULL; }
    size_t i = 4, end = (size_t)len - 1;
    while (i < end) {
        uint8_t t = p[i++];
        const uint8_t *z = memchr(p + i, 0, end - i);
        if (!z) { fail(err, "BSON: a key past the end of its document"); goto bad; }
        const char *key = (const char *)p + i;
        size_t klen = (size_t)(z - (p + i));
        if (!utf8_ok(p + i, klen)) { fail(err, "BSON: a key not of UTF-8"); goto bad; }
        i += klen + 1;
        size_t n = 0;
        GenericValue *v = read_value(t, p + i, end - i, ext, depth, &n, err);
        if (!v) { fail(err, "BSON: out of memory"); goto bad; }
        i += n;
        if (!(array ? arr_push(g, v) : map_put(g, key, klen, v))) { fail(err, "BSON: out of memory"); goto bad; }
    }
    if (i != end) { fail(err, "BSON: a value past the end of its document"); goto bad; }
    *used = (size_t)len;
    return g;
bad:
    gv_free(g);
    return NULL;
}

GenericValue *gv_from_bson(const uint8_t *data, size_t len, DocError *out_err) {
    if (out_err) out_err->msg = NULL;
    size_t used;
    return read_doc(data, len, 0, 0, 0, &used, out_err);
}

/* ═══════════════════════════════════════════════════════════════════════
 *  GenericValue → BSON
 * ═══════════════════════════════════════════════════════════════════════ */

typedef struct {
    uint8_t *p;
    size_t   len, cap;
    int      oom;
    /* the path to the value written, as the errors show it: "/a/1" */
    char    *at;
    size_t   at_len, at_cap;
    DocError *err;
} Out;

static void put(Out *o, const void *b, size_t n) {
    if (o->oom) return;
    if (o->len + n > o->cap) {
        size_t cap = o->cap ? o->cap : 256;
        while (cap < o->len + n) cap *= 2;
        uint8_t *p = realloc(o->p, cap);
        if (!p) { o->oom = 1; return; }
        o->p = p;
        o->cap = cap;
    }
    memcpy(o->p + o->len, b, n);
    o->len += n;
}

static void put_u8(Out *o, uint8_t v) { put(o, &v, 1); }

static void put_i32(Out *o, int32_t v) {
    uint8_t b[4] = { (uint8_t)v, (uint8_t)(v >> 8), (uint8_t)(v >> 16), (uint8_t)((uint32_t)v >> 24) };
    put(o, b, 4);
}

static void put_u64(Out *o, uint64_t v) {
    uint8_t b[8];
    put_le64(b, v);
    put(o, b, 8);
}

static void put_cstr(Out *o, const char *s) { put(o, s, strlen(s) + 1); }

static void put_str(Out *o, const char *s) {
    put_i32(o, (int32_t)(strlen(s) + 1));
    put_cstr(o, s);
}

static void patch_i32(Out *o, size_t at, size_t v) {
    if (o->oom) return;
    if (v > INT32_MAX) { o->oom = 1; return; }
    for (int i = 0; i < 4; i++) o->p[at + i] = (uint8_t)(v >> 8 * i);
}

/* the path one key deeper; returns its length before, to pop to */
static size_t at_push(Out *o, const char *key) {
    size_t before = o->at_len, n = strlen(key);
    if (o->at_len + n + 2 > o->at_cap) {
        size_t cap = (o->at_len + n + 2) * 2;
        char *a = realloc(o->at, cap);
        if (!a) { o->oom = 1; return before; }
        o->at = a;
        o->at_cap = cap;
    }
    o->at[o->at_len++] = '/';
    memcpy(o->at + o->at_len, key, n + 1);
    o->at_len += n;
    return before;
}

static void at_pop(Out *o, size_t before) {
    o->at_len = before;
    if (o->at) o->at[before] = 0;
}

static const char *at_str(Out *o) { return o->at ? o->at : ""; }

/* the keys of extended JSON's types, in the order the bson crate looks for them */
static const char *const TYPES[] = { "$oid", "$symbol", "$regularExpression", "$numberInt", "$numberLong",
    "$numberDouble", "$numberDecimal", "$binary", "$uuid", "$code", "$timestamp", "$date", "$minKey", "$maxKey",
    "$dbPointer", "$undefined" };
#define N_TYPES (sizeof TYPES / sizeof *TYPES)

static const GenericValue *get(const GenericValue *m, const char *key) {
    if (!m || m->kind != GV_MAP) return NULL;
    for (size_t i = 0; i < m->map.len; i++)
        if (strcmp(m->map.ents[i].key, key) == 0) return m->map.ents[i].val;
    return NULL;
}

/* the type key of a map of extended JSON's, or NULL: a document */
static const char *type_key(const GenericValue *m) {
    for (size_t i = 0; i < N_TYPES; i++)
        if (get(m, TYPES[i])) return TYPES[i];
    return NULL;
}

/* a string of extended JSON's: from_json made a number of it, when it reads as one */
static const char *text(const GenericValue *v) {
    return v && (v->kind == GV_STRING || v->kind == GV_NUMERIC) ? v->string : NULL;
}

/* a map of these keys and none else */
static int only(const GenericValue *m, size_t n) { return m && m->kind == GV_MAP && m->map.len == n; }

static int cmp_key(const void *a, const void *b) {
    return strcmp((*(const MapEntry *const *)a)->key, (*(const MapEntry *const *)b)->key);
}

static int cmp_root_key(const void *a, const void *b) {
    const char *x = (*(const MapEntry *const *)a)->key, *y = (*(const MapEntry *const *)b)->key;
    int ix = strcmp(x, "_id") == 0, iy = strcmp(y, "_id") == 0;
    return ix != iy ? iy - ix : strcmp(x, y);
}

static uint8_t write_value(Out *o, const GenericValue *v);

/* a document, its keys sorted - the root's _id first; or an array */
static int write_doc(Out *o, const GenericValue *v, int root) {
    size_t start = o->len;
    put_i32(o, 0);
    if (v->kind == GV_ARRAY) {
        for (size_t i = 0; i < v->arr.len; i++) {
            char key[24];
            snprintf(key, sizeof key, "%zu", i);
            size_t type_at = o->len;
            put_u8(o, 0);
            put_cstr(o, key);
            size_t before = at_push(o, key);
            uint8_t t = write_value(o, v->arr.items[i]);
            at_pop(o, before);
            if (!t) return 0;
            if (!o->oom) o->p[type_at] = t;
        }
    } else {
        const MapEntry **ents = malloc((v->map.len ? v->map.len : 1) * sizeof *ents);
        if (!ents) { o->oom = 1; return 0; }
        for (size_t i = 0; i < v->map.len; i++) ents[i] = &v->map.ents[i];
        qsort(ents, v->map.len, sizeof *ents, root ? cmp_root_key : cmp_key);
        for (size_t i = 0; i < v->map.len; i++) {
            size_t type_at = o->len;
            put_u8(o, 0);
            put_cstr(o, ents[i]->key);
            size_t before = at_push(o, ents[i]->key);
            uint8_t t = write_value(o, ents[i]->val);
            at_pop(o, before);
            if (!t) { free(ents); return 0; }
            if (!o->oom) o->p[type_at] = t;
        }
        free(ents);
    }
    put_u8(o, 0);
    patch_i32(o, start, o->len - start);
    return 1;
}

/* a value of extended JSON's, `key` its type: the bson crate's reading of it */
static uint8_t write_ext(Out *o, const GenericValue *m, const char *key) {
    const GenericValue *v = get(m, key);
    const char *s = text(v);
    uint8_t b[16];
    int64_t i;
    double d;
    uint32_t t, inc;
#define BAD(what) do { fail(o->err, "BSON: %s at %s", what, at_str(o)); return 0; } while (0)
    if (strcmp(key, "$code") == 0) {
        const GenericValue *scope = get(m, "$scope");
        if (!s || m->map.len != (scope ? 2u : 1u)) BAD("not a $code");
        if (!scope) { put_str(o, s); return 0x0D; }
        if (scope->kind != GV_MAP || type_key(scope)) {
            fail(o->err, "DD-E501 a BSON document is a map, not a value of extended JSON nor of another type");
            return 0;
        }
        size_t start = o->len;
        put_i32(o, 0);
        put_str(o, s);
        if (!write_doc(o, scope, 0)) return 0;
        patch_i32(o, start, o->len - start);
        return 0x0F;
    }
    if (m->map.len != 1) BAD("keys beside the type of extended JSON");
    if (strcmp(key, "$oid") == 0) {
        if (!s || !hex_to(s, b, 12)) BAD("not an ObjectId");
        put(o, b, 12);
        return 0x07;
    }
    if (strcmp(key, "$symbol") == 0) {
        if (!s) BAD("not a $symbol");
        put_str(o, s);
        return 0x0E;
    }
    if (strcmp(key, "$regularExpression") == 0) {
        const char *pat = text(get(v, "pattern")), *opt = text(get(v, "options"));
        if (!only(v, 2) || !pat || !opt) BAD("not a $regularExpression");
        size_t n = strlen(opt);
        char *sorted = dup_n(opt, n);
        if (!sorted) { o->oom = 1; return 0; }
        for (size_t k = 1; k < n; k++)
            for (size_t j = k; j > 0 && (uint8_t)sorted[j - 1] > (uint8_t)sorted[j]; j--) {
                char c = sorted[j]; sorted[j] = sorted[j - 1]; sorted[j - 1] = c;
            }
        put_cstr(o, pat);
        put_cstr(o, sorted);
        free(sorted);
        return 0x0B;
    }
    if (strcmp(key, "$numberInt") == 0) {
        if (!s || !parse_i64(s, &i) || i < INT32_MIN || i > INT32_MAX) BAD("not a $numberInt");
        put_i32(o, (int32_t)i);
        return 0x10;
    }
    if (strcmp(key, "$numberLong") == 0) {
        if (!s || !parse_i64(s, &i)) BAD("not a $numberLong");
        put_u64(o, (uint64_t)i);
        return 0x12;
    }
    if (strcmp(key, "$numberDouble") == 0) {
        if (!s) BAD("not a $numberDouble");
        if (strcmp(s, "Infinity") == 0) d = HUGE_VAL;
        else if (strcmp(s, "-Infinity") == 0) d = -HUGE_VAL;
        else if (strcmp(s, "NaN") == 0) d = NAN;
        else if (!parse_f64(s, &d)) BAD("not a $numberDouble");
        uint64_t bits;
        memcpy(&bits, &d, sizeof bits);
        put_u64(o, bits);
        return 0x01;
    }
    if (strcmp(key, "$numberDecimal") == 0) {
        if (!s || !dec_from_str(s, b)) BAD("not a $numberDecimal");
        put(o, b, 16);
        return 0x13;
    }
    if (strcmp(key, "$binary") == 0 || strcmp(key, "$uuid") == 0) {
        uint8_t *bytes = NULL, sub = 4;
        size_t n = 16;
        if (key[1] == 'b') {
            const char *b64 = text(get(v, "base64")), *st = text(get(v, "subType"));
            if (!only(v, 2) || !b64 || !st || !hex_to(st, &sub, 1) || !base64_to(b64, &bytes, &n)) BAD("not a $binary");
        } else {
            /* hyphenated, simple, braced or urn: as the uuid crate reads it */
            char hex[33];
            size_t k = 0, len = s ? strlen(s) : 0;
            const char *u = s;
            if (u && len == 45 && strncmp(u, "urn:uuid:", 9) == 0) { u += 9; len -= 9; }
            else if (u && len == 38 && u[0] == '{' && u[37] == '}') { u++; len -= 2; }
            for (size_t j = 0; u && j < len && k < 33; j++) {
                if (len == 36 && (j == 8 || j == 13 || j == 18 || j == 23)) { if (u[j] != '-') { k = 99; break; } continue; }
                hex[k++] = u[j];
            }
            if (!u || (len != 36 && len != 32) || k != 32) BAD("not a $uuid");
            hex[32] = 0;
            if (!hex_to(hex, b, 16)) BAD("not a $uuid");
            bytes = malloc(16);
            if (!bytes) { o->oom = 1; return 0; }
            memcpy(bytes, b, 16);
        }
        if (sub == 0x02) {
            put_i32(o, (int32_t)n + 4);
            put_u8(o, sub);
            put_i32(o, (int32_t)n);
        } else {
            put_i32(o, (int32_t)n);
            put_u8(o, sub);
        }
        put(o, bytes, n);
        free(bytes);
        return 0x05;
    }
    if (strcmp(key, "$timestamp") == 0) {
        const GenericValue *tv = get(v, "t"), *iv = get(v, "i");
        if (!only(v, 2) || !tv || !iv || tv->kind != GV_NUMERIC || iv->kind != GV_NUMERIC
            || !parse_u32(tv->string, &t) || !parse_u32(iv->string, &inc)) BAD("not a $timestamp");
        put_u64(o, (uint64_t)t << 32 | inc);
        return 0x11;
    }
    if (strcmp(key, "$date") == 0) {
        const char *ms = text(get(v, "$numberLong"));
        if (s ? !parse_date(s, &i) : !(only(v, 1) && ms && parse_i64(ms, &i))) BAD("not a $date");
        put_u64(o, (uint64_t)i);
        return 0x09;
    }
    if (strcmp(key, "$minKey") == 0 || strcmp(key, "$maxKey") == 0) {
        if (!v || v->kind != GV_NUMERIC || !parse_i64(v->string, &i) || i != 1) BAD("not a $minKey or $maxKey");
        return key[2] == 'i' ? 0xFF : 0x7F;
    }
    if (strcmp(key, "$dbPointer") == 0) {
        const char *ns = text(get(v, "$ref"));
        const GenericValue *id = get(v, "$id");
        const char *oid = text(get(id, "$oid"));
        if (!only(v, 2) || !ns || !only(id, 1) || !oid || !hex_to(oid, b, 12)) BAD("not a $dbPointer");
        put_str(o, ns);
        put(o, b, 12);
        return 0x0C;
    }
    /* $undefined */
    if (!v || v->kind != GV_BOOL || !v->boolean) BAD("not an $undefined");
    return 0x06;
#undef BAD
}

/* a value's bytes, and its BSON type; 0 on error */
static uint8_t write_value(Out *o, const GenericValue *v) {
    int64_t i;
    double d;
    switch (v->kind) {
    case GV_NUMERIC:
        if (parse_i64(v->string, &i) && i >= INT32_MIN && i <= INT32_MAX) { put_i32(o, (int32_t)i); return 0x10; }
        if (parse_i64(v->string, &i)) { put_u64(o, (uint64_t)i); return 0x12; }
        /* an integer past int64: as a double it would lose its digits */
        if (int_syntax(v->string) || !parse_f64(v->string, &d)) {
            fail(o->err, "DD-E502 the number %s at %s has no BSON type: it is past int64", v->string, at_str(o));
            return 0;
        }
        {
            uint64_t bits;
            memcpy(&bits, &d, sizeof bits);
            put_u64(o, bits);
        }
        return 0x01;
    case GV_STRING:
        put_str(o, v->string);
        return 0x02;
    case GV_BOOL:
        put_u8(o, v->boolean ? 1 : 0);
        return 0x08;
    case GV_NULL:
        return 0x0A;
    case GV_ARRAY:
        return write_doc(o, v, 0) ? 0x04 : 0;
    case GV_MAP: {
        const char *key = type_key(v);
        if (key) return write_ext(o, v, key);
        return write_doc(o, v, 0) ? 0x03 : 0;
    }
    }
    return 0;
}

int gv_to_bson(const GenericValue *gv, uint8_t **out, size_t *out_len, DocError *out_err) {
    if (out_err) out_err->msg = NULL;
    *out = NULL;
    *out_len = 0;
    if (!gv || gv->kind != GV_MAP || type_key(gv)) {
        fail(out_err, "DD-E501 a BSON document is a map, not a value of extended JSON nor of another type");
        return -1;
    }
    Out o = {0};
    o.err = out_err;
    int ok = write_doc(&o, gv, 1);
    free(o.at);
    if (ok && o.oom) fail(out_err, "BSON: out of memory, or a document past 2 GiB");
    if (!ok || o.oom) {
        free(o.p);
        return -1;
    }
    *out = o.p;
    *out_len = o.len;
    return 0;
}
