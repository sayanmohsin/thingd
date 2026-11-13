# ThingDB for Go

This Go module is a cgo wrapper around ThingDB's C ABI. It embeds the database
in the Go process and does not start Thingd or a network service.

Install the Go module after a Thingd release with
`go get github.com/sayanmohsin/thingd/bindings/go@v0.92.9` (replace the version
with the matching release). Release automation creates the nested-module tag.

Build the C ABI library from the Thingd repository, then point cgo at the
header/library when building a Go consumer:

```sh
cargo build --release -p thingdb-ffi
cd bindings/go
CGO_LDFLAGS="-L$PWD/../../target/release" \
LD_LIBRARY_PATH="$PWD/../../target/release" \
go test ./...
```

In a consumer, install/link the matching `thingdb-ffi` shared library and use:

```go
db, err := thingdb.Open("./data")
if err != nil {
    return err
}
defer db.Close()

if err := db.Put("users", []byte("alice"), []byte("hello")); err != nil {
    return err
}
value, found, err := db.Get("users", []byte("alice"))
```

The `include/thingdb.h` file mirrors the canonical header in
`bindings/thingdb-ffi/include`; CI checks that they remain identical.
Durable files and the C ABI remain experimental until ThingDB 1.0.
