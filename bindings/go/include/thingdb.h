#ifndef THINGDB_H
#define THINGDB_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct ThingDbHandle ThingDbHandle;

typedef struct ThingDbBuffer {
  uint8_t *data;
  size_t length;
} ThingDbBuffer;

typedef enum ThingDbStatus {
  THINGDB_OK = 0,
  THINGDB_INVALID_ARGUMENT = 1,
  THINGDB_STORAGE_ERROR = 2,
  THINGDB_PANIC = 3
} ThingDbStatus;

/* Open a durable path or ":memory:". `out_db` must point to writable storage. */
ThingDbStatus thingdb_open(const uint8_t *path, size_t path_length,
                           ThingDbHandle **out_db);

/* Close a handle returned by thingdb_open. Do not call concurrently with use. */
ThingDbStatus thingdb_close(ThingDbHandle *db);

ThingDbStatus thingdb_put(ThingDbHandle *db, const uint8_t *keyspace,
                          size_t keyspace_length, const uint8_t *key,
                          size_t key_length, const uint8_t *value,
                          size_t value_length);

/* `found` distinguishes a missing key from a stored zero-length value. */
ThingDbStatus thingdb_get(ThingDbHandle *db, const uint8_t *keyspace,
                          size_t keyspace_length, const uint8_t *key,
                          size_t key_length, bool *found,
                          ThingDbBuffer *out_value);

ThingDbStatus thingdb_delete(ThingDbHandle *db, const uint8_t *keyspace,
                             size_t keyspace_length, const uint8_t *key,
                             size_t key_length);

/* Free a buffer returned by thingdb_get. */
void thingdb_buffer_free(ThingDbBuffer buffer);

/* Thread-local diagnostic for the last non-OK call; valid until next call. */
const char *thingdb_last_error(void);

#ifdef __cplusplus
}
#endif

#endif
