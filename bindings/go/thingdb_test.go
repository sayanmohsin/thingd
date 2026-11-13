package thingdb

import (
	"bytes"
	"testing"
)

func TestOpenPutGetDelete(t *testing.T) {
	db, err := Open(":memory:")
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()

	if err := db.Put("users", []byte("alice"), []byte("value")); err != nil {
		t.Fatal(err)
	}
	value, found, err := db.Get("users", []byte("alice"))
	if err != nil || !found || !bytes.Equal(value, []byte("value")) {
		t.Fatalf("Get() = %q, %v, %v", value, found, err)
	}
	if err := db.Put("users", []byte("empty"), []byte{}); err != nil {
		t.Fatal(err)
	}
	value, found, err = db.Get("users", []byte("empty"))
	if err != nil || !found || len(value) != 0 {
		t.Fatalf("empty Get() = %q, %v, %v", value, found, err)
	}
	if err := db.Delete("users", []byte("alice")); err != nil {
		t.Fatal(err)
	}
	_, found, err = db.Get("users", []byte("alice"))
	if err != nil || found {
		t.Fatalf("deleted Get() found=%v err=%v", found, err)
	}
}
