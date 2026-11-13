package thingdb

/*
#cgo CFLAGS: -I${SRCDIR}/include
#cgo LDFLAGS: -lthingdb_ffi
#include <stdlib.h>
#include "thingdb.h"
*/
import "C"

import (
	"errors"
	"fmt"
	"runtime"
	"unsafe"
)

// DB is a handle to an embedded ThingDB database. Do not call Close while
// another goroutine is using this value.
type DB struct {
	handle *C.ThingDbHandle
}

// Open opens a durable database path, or a process-local database for ":memory:".
func Open(path string) (*DB, error) {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	bytes := []byte(path)
	var pointer *C.uint8_t
	if len(bytes) > 0 {
		pointer = (*C.uint8_t)(unsafe.Pointer(&bytes[0]))
	}
	var handle *C.ThingDbHandle
	status := C.thingdb_open(pointer, C.size_t(len(bytes)), &handle)
	runtime.KeepAlive(bytes)
	if status != C.THINGDB_OK {
		return nil, lastError(status)
	}
	return &DB{handle: handle}, nil
}

// Put stores a copied byte key and value under the named keyspace.
func (db *DB) Put(keyspace string, key, value []byte) error {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	if err := db.checkOpen(); err != nil {
		return err
	}
	space := []byte(keyspace)
	status := C.thingdb_put(
		db.handle,
		bytePointer(space), C.size_t(len(space)),
		bytePointer(key), C.size_t(len(key)),
		bytePointer(value), C.size_t(len(value)),
	)
	runtime.KeepAlive(space)
	runtime.KeepAlive(key)
	runtime.KeepAlive(value)
	if status != C.THINGDB_OK {
		return lastError(status)
	}
	return nil
}

// Get returns a copied value and whether the key exists. Empty stored values
// are distinguishable from missing keys.
func (db *DB) Get(keyspace string, key []byte) ([]byte, bool, error) {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	if err := db.checkOpen(); err != nil {
		return nil, false, err
	}
	space := []byte(keyspace)
	var found C.bool
	var output C.ThingDbBuffer
	status := C.thingdb_get(
		db.handle,
		bytePointer(space), C.size_t(len(space)),
		bytePointer(key), C.size_t(len(key)), &found, &output,
	)
	runtime.KeepAlive(space)
	runtime.KeepAlive(key)
	if status != C.THINGDB_OK {
		return nil, false, lastError(status)
	}
	defer C.thingdb_buffer_free(output)
	if !bool(found) {
		return nil, false, nil
	}
	if output.length == 0 {
		return []byte{}, true, nil
	}
	if uint64(output.length) > uint64(^uint(0)>>1) {
		return nil, false, errors.New("ThingDB value exceeds Go slice limits")
	}
	value := append([]byte(nil), unsafe.Slice((*byte)(unsafe.Pointer(output.data)), int(output.length))...)
	return value, true, nil
}

// Delete removes a key from a keyspace.
func (db *DB) Delete(keyspace string, key []byte) error {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	if err := db.checkOpen(); err != nil {
		return err
	}
	space := []byte(keyspace)
	status := C.thingdb_delete(db.handle, bytePointer(space), C.size_t(len(space)), bytePointer(key), C.size_t(len(key)))
	runtime.KeepAlive(space)
	runtime.KeepAlive(key)
	if status != C.THINGDB_OK {
		return lastError(status)
	}
	return nil
}

// Close releases the database handle. Calling Close more than once returns an error.
func (db *DB) Close() error {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	if err := db.checkOpen(); err != nil {
		return err
	}
	status := C.thingdb_close(db.handle)
	db.handle = nil
	if status != C.THINGDB_OK {
		return lastError(status)
	}
	return nil
}

func (db *DB) checkOpen() error {
	if db == nil || db.handle == nil {
		return errors.New("ThingDB is closed")
	}
	return nil
}

func bytePointer(value []byte) *C.uint8_t {
	if len(value) == 0 {
		return nil
	}
	return (*C.uint8_t)(unsafe.Pointer(&value[0]))
}

func lastError(status C.ThingDbStatus) error {
	message := C.GoString(C.thingdb_last_error())
	if message == "" {
		return fmt.Errorf("ThingDB operation failed with status %d", uint32(status))
	}
	return fmt.Errorf("ThingDB status %d: %s", uint32(status), message)
}
