/* C smoke test for the dusrelimage FFI library.
 *
 * Compiled with gcc against the built cdylib and `dusrelimage.h`
 * (see `run_c_smoke.sh`). Exercises the four calls a new Go/PHP
 * binding would hit first: version, compress, get_image_info and
 * free_buffer — plus an error path (garbage bytes -> INVALID_IMAGE
 * with a JSON last error).
 *
 * Usage: smoke <fixture.png>
 * Exit code 0 on success, non-zero with a message on stderr on failure.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "dusrelimage.h"

static int failures = 0;

#define CHECK(cond, ...)                                                     \
    do {                                                                     \
        if (!(cond)) {                                                       \
            fprintf(stderr, "FAIL: " __VA_ARGS__);                           \
            fprintf(stderr, "\n");                                           \
            failures++;                                                      \
        }                                                                    \
    } while (0)

static uint8_t *read_file(const char *path, size_t *len_out) {
    FILE *f = fopen(path, "rb");
    if (!f) {
        fprintf(stderr, "cannot open %s\n", path);
        exit(2);
    }
    fseek(f, 0, SEEK_END);
    long len = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (len < 0) {
        fclose(f);
        exit(2);
    }
    uint8_t *buf = malloc((size_t)len > 0 ? (size_t)len : 1);
    if (!buf) {
        fclose(f);
        exit(2);
    }
    if (len > 0 && fread(buf, 1, (size_t)len, f) != (size_t)len) {
        fclose(f);
        free(buf);
        exit(2);
    }
    fclose(f);
    *len_out = (size_t)len;
    return buf;
}

static char *dup_nul(const uint8_t *p, size_t len) {
    /* FFI JSON outputs are NOT NUL-terminated: copy before using
     * any C string function on them. */
    char *s = malloc(len + 1);
    if (!s) {
        exit(2);
    }
    memcpy(s, p, len);
    s[len] = '\0';
    return s;
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <fixture.png>\n", argv[0]);
        return 2;
    }

    /* version */
    const char *version = dusrelimage_version();
    CHECK(version != NULL, "version returned NULL");
    CHECK(version && strcmp(version, "0.1.0") == 0, "version is %s, want 0.1.0",
          version ? version : "(null)");
    printf("version: %s\n", version ? version : "(null)");

    size_t input_len = 0;
    uint8_t *input = read_file(argv[1], &input_len);
    printf("fixture: %s (%zu bytes)\n", argv[1], input_len);

    /* get_image_info (NULL options = defaults) */
    uint8_t *info_json = NULL;
    size_t info_len = 0;
    int32_t status =
        dusrelimage_get_image_info(input, input_len, NULL, &info_json, &info_len);
    CHECK(status == DUSRELIMAGE_OK, "get_image_info status %d, want 0", status);
    if (status == DUSRELIMAGE_OK) {
        char *info_text = dup_nul(info_json, info_len);
        printf("info: %s\n", info_text);
        CHECK(strstr(info_text, "\"format\"") != NULL,
              "info JSON has no format field");
        free(info_text);
        dusrelimage_free_buffer(info_json, info_len);
    }

    /* compress to JPEG at quality 70 (partial options JSON) */
    uint8_t *out = NULL;
    size_t out_len = 0;
    status = dusrelimage_compress(input, input_len, "{\"format\": \"Jpeg\", \"quality\": 70}",
                                  &out, &out_len);
    CHECK(status == DUSRELIMAGE_OK, "compress status %d, want 0", status);
    if (status == DUSRELIMAGE_OK) {
        CHECK(out_len > 2 && out[0] == 0xFF && out[1] == 0xD8,
              "compress output is not a JPEG (len %zu)", out_len);
        printf("compress: %zu bytes -> %zu bytes JPEG\n", input_len, out_len);
        dusrelimage_free_buffer(out, out_len);
    }

    /* error path: garbage bytes -> INVALID_IMAGE + JSON last error */
    const uint8_t garbage[] = "this is not an image at all";
    out = NULL;
    out_len = 0;
    status = dusrelimage_compress(garbage, sizeof(garbage) - 1, NULL, &out, &out_len);
    CHECK(status == DUSRELIMAGE_INVALID_IMAGE, "garbage status %d, want 1", status);
    CHECK(out == NULL, "failed call returned a buffer");
    const char *err = dusrelimage_last_error();
    CHECK(err != NULL, "last_error is NULL after a failure");
    CHECK(err && strstr(err, "INVALID_IMAGE") != NULL, "last_error: %s",
          err ? err : "(null)");
    printf("last_error: %s\n", err ? err : "(null)");

    /* a successful call clears the last error */
    status = dusrelimage_get_image_info(input, input_len, NULL, &info_json, &info_len);
    CHECK(status == DUSRELIMAGE_OK, "second get_image_info status %d", status);
    if (status == DUSRELIMAGE_OK) {
        dusrelimage_free_buffer(info_json, info_len);
    }
    CHECK(dusrelimage_last_error() == NULL, "last_error not cleared after success");

    free(input);
    if (failures == 0) {
        printf("C smoke test: PASS\n");
        return 0;
    }
    printf("C smoke test: %d FAILURE(S)\n", failures);
    return 1;
}
