//! C ABI for the standalone `ThingDB` embedded key/value database.

#![allow(unsafe_code)] // Raw pointers are required by the documented C ABI.
#![allow(clippy::borrow_as_ptr)] // Tests pass writable raw C output pointers.

use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::slice;

use thingdb::{Database, KeyspaceCreateOptions};

/// Opaque C handle to an open `ThingDB` database.
pub struct ThingDbHandle {
    database: Database,
}

/// Byte buffer allocated by `ThingDB` and freed with [`thingdb_buffer_free`].
#[repr(C)]
pub struct ThingDbBuffer {
    /// Pointer to owned bytes, or null when the value is empty or missing.
    pub data: *mut u8,
    /// Number of bytes at `data`.
    pub length: usize,
}

/// Stable status values returned by C ABI functions for the current release line.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThingDbStatus {
    /// Operation succeeded.
    Ok = 0,
    /// An input pointer, length, or UTF-8 string was invalid.
    InvalidArgument = 1,
    /// The underlying database operation failed.
    StorageError = 2,
    /// A Rust panic was caught at the C boundary.
    Panic = 3,
}

thread_local! {
    static LAST_ERROR: std::cell::RefCell<CString> = std::cell::RefCell::new(CString::default());
}

fn set_error(message: &str) {
    let clean = message.replace('\0', "?");
    let message = CString::new(clean).unwrap_or_default();
    LAST_ERROR.with(|last_error| *last_error.borrow_mut() = message);
}

fn clear_error() {
    LAST_ERROR.with(|last_error| *last_error.borrow_mut() = CString::default());
}

fn boundary(operation: impl FnOnce() -> Result<(), (ThingDbStatus, String)>) -> ThingDbStatus {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(())) => {
            clear_error();
            ThingDbStatus::Ok
        },
        Ok(Err((status, message))) => {
            set_error(&message);
            status
        },
        Err(_) => {
            set_error("ThingDB caught a panic at the C ABI boundary");
            ThingDbStatus::Panic
        },
    }
}

fn invalid(message: &str) -> (ThingDbStatus, String) {
    (ThingDbStatus::InvalidArgument, message.to_owned())
}

fn storage(error: impl std::fmt::Display) -> (ThingDbStatus, String) {
    (ThingDbStatus::StorageError, error.to_string())
}

unsafe fn input_bytes<'a>(
    data: *const u8,
    length: usize,
) -> Result<&'a [u8], (ThingDbStatus, String)> {
    if length == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(invalid("non-empty byte input has a null pointer"));
    }
    // SAFETY: The C caller guarantees `data` is valid for `length` bytes.
    Ok(unsafe { slice::from_raw_parts(data, length) })
}

unsafe fn keyspace(
    db: *const ThingDbHandle,
    name: *const u8,
    name_length: usize,
) -> Result<thingdb::Keyspace, (ThingDbStatus, String)> {
    if db.is_null() {
        return Err(invalid("database handle is null"));
    }
    // SAFETY: The C caller guarantees `db` points to a live handle.
    let database = unsafe { &*db };
    // SAFETY: The C caller guarantees the input pointer and length pair is valid.
    let name = unsafe { input_bytes(name, name_length)? };
    let name = std::str::from_utf8(name).map_err(|error| invalid(&error.to_string()))?;
    database
        .database
        .keyspace(name, KeyspaceCreateOptions::default)
        .map_err(storage)
}

/// Open a durable path or the special `:memory:` database.
///
/// # Safety
/// `path` must point to `path_length` readable bytes when the length is non-zero.
/// `out_db` must point to writable storage for a `ThingDbHandle` pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_open(
    path: *const u8,
    path_length: usize,
    out_db: *mut *mut ThingDbHandle,
) -> ThingDbStatus {
    if out_db.is_null() {
        set_error("out_db pointer is null");
        return ThingDbStatus::InvalidArgument;
    }
    // SAFETY: Checked non-null above; the caller supplies writable output storage.
    unsafe { out_db.write(std::ptr::null_mut()) };
    boundary(|| {
        // SAFETY: The C caller guarantees the input pointer/length pair is valid.
        let path = unsafe { input_bytes(path, path_length)? };
        let path = std::str::from_utf8(path).map_err(|error| invalid(&error.to_string()))?;
        if path.is_empty() {
            return Err(invalid("database path must not be empty"));
        }
        let database = if path == ":memory:" {
            Database::in_memory()
        } else {
            Database::open(path)
        }
        .map_err(storage)?;
        let handle = Box::new(ThingDbHandle { database });
        // SAFETY: `out_db` was checked and is writable by the caller contract.
        unsafe { out_db.write(Box::into_raw(handle)) };
        Ok(())
    })
}

/// Close an open database handle.
///
/// # Safety
/// `db` must be null or a live handle returned by [`thingdb_open`], and no other
/// thread may be using the handle during this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_close(db: *mut ThingDbHandle) -> ThingDbStatus {
    if db.is_null() {
        set_error("database handle is null");
        return ThingDbStatus::InvalidArgument;
    }
    boundary(|| {
        // SAFETY: The caller guarantees this is a live handle and does not use it concurrently.
        drop(unsafe { Box::from_raw(db) });
        Ok(())
    })
}

/// Insert or replace an opaque key/value pair.
///
/// # Safety
/// All input pointer/length pairs must reference readable memory for their lengths.
/// `db` must be a live handle and must not be concurrently closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_put(
    db: *const ThingDbHandle,
    namespace: *const u8,
    namespace_length: usize,
    key: *const u8,
    key_length: usize,
    value: *const u8,
    value_length: usize,
) -> ThingDbStatus {
    boundary(|| {
        // SAFETY: The C caller guarantees handle and buffer lifetimes for this call.
        let keyspace = unsafe { keyspace(db, namespace, namespace_length)? };
        // SAFETY: The C caller guarantees the input pointer/length pairs are valid.
        let key = unsafe { input_bytes(key, key_length)? };
        // SAFETY: The C caller guarantees the input pointer/length pairs are valid.
        let value = unsafe { input_bytes(value, value_length)? };
        keyspace.insert(key, value).map_err(storage)
    })
}

/// Read an opaque value and return owned bytes in `out_value`.
///
/// # Safety
/// All input pointer/length pairs must reference readable memory for their lengths.
/// `found` and `out_value` must be writable, and `db` must remain live during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_get(
    db: *const ThingDbHandle,
    namespace: *const u8,
    namespace_length: usize,
    key: *const u8,
    key_length: usize,
    found: *mut bool,
    out_value: *mut ThingDbBuffer,
) -> ThingDbStatus {
    if found.is_null() || out_value.is_null() {
        set_error("found and out_value must point to writable storage");
        return ThingDbStatus::InvalidArgument;
    }
    // SAFETY: Non-null checked; caller guarantees writable output pointers.
    unsafe {
        found.write(false);
        out_value.write(ThingDbBuffer {
            data: std::ptr::null_mut(),
            length: 0,
        });
    }
    boundary(|| {
        // SAFETY: The C caller guarantees handle and buffer lifetimes for this call.
        let keyspace = unsafe { keyspace(db, namespace, namespace_length)? };
        // SAFETY: The C caller guarantees the input pointer/length pair is valid.
        let key = unsafe { input_bytes(key, key_length)? };
        let value = keyspace.get(key).map_err(storage)?;
        if let Some(value) = value {
            // SAFETY: Non-null checked; caller guarantees writable output pointers.
            unsafe { found.write(true) };
            if !value.is_empty() {
                let boxed = value.into_boxed_slice();
                let length = boxed.len();
                let data = Box::into_raw(boxed).cast::<u8>();
                // SAFETY: Non-null checked; caller guarantees writable output pointers.
                unsafe { out_value.write(ThingDbBuffer { data, length }) };
            }
        }
        Ok(())
    })
}

/// Remove a key if it exists.
///
/// # Safety
/// All input pointer/length pairs must reference readable memory for their lengths.
/// `db` must be a live handle and must not be concurrently closed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_delete(
    db: *const ThingDbHandle,
    namespace: *const u8,
    namespace_length: usize,
    key: *const u8,
    key_length: usize,
) -> ThingDbStatus {
    boundary(|| {
        // SAFETY: The C caller guarantees handle and buffer lifetimes for this call.
        let keyspace = unsafe { keyspace(db, namespace, namespace_length)? };
        // SAFETY: The C caller guarantees the input pointer/length pair is valid.
        let key = unsafe { input_bytes(key, key_length)? };
        keyspace.remove(key).map_err(storage)
    })
}

/// Release a value buffer returned by [`thingdb_get`].
///
/// # Safety
/// `buffer.data` must be null or the unchanged buffer returned by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_buffer_free(buffer: ThingDbBuffer) {
    if !buffer.data.is_null() {
        // SAFETY: The pointer was allocated as a boxed slice by `thingdb_get`; length is preserved.
        let slice = std::ptr::slice_from_raw_parts_mut(buffer.data, buffer.length);
        // SAFETY: The pointer/length pair was returned by this library and is freed once.
        drop(unsafe { Box::from_raw(slice) });
    }
}

/// Return the current thread's last diagnostic string.
///
/// # Safety
/// The returned pointer is borrowed and must not be freed. It remains valid until
/// the next `ThingDB` C ABI call on the same thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn thingdb_last_error() -> *const c_char {
    LAST_ERROR.with(|last_error| last_error.borrow().as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_abi_round_trip_and_owned_buffer() {
        let path = b":memory:";
        let mut db = std::ptr::null_mut();
        // SAFETY: Pointers refer to valid test buffers and writable output slots.
        assert_eq!(
            unsafe { thingdb_open(path.as_ptr(), path.len(), &raw mut db) },
            ThingDbStatus::Ok
        );
        let namespace = b"test";
        let key = b"key";
        let value = b"value";
        // SAFETY: Pointers refer to valid test buffers and the live handle.
        assert_eq!(
            unsafe {
                thingdb_put(
                    db,
                    namespace.as_ptr(),
                    namespace.len(),
                    key.as_ptr(),
                    key.len(),
                    value.as_ptr(),
                    value.len(),
                )
            },
            ThingDbStatus::Ok
        );
        let mut found = false;
        let mut output = ThingDbBuffer {
            data: std::ptr::null_mut(),
            length: 0,
        };
        // SAFETY: Pointers refer to valid test buffers and writable output slots.
        assert_eq!(
            unsafe {
                thingdb_get(
                    db,
                    namespace.as_ptr(),
                    namespace.len(),
                    key.as_ptr(),
                    key.len(),
                    &raw mut found,
                    &raw mut output,
                )
            },
            ThingDbStatus::Ok
        );
        assert!(found);
        // SAFETY: The output buffer came from the preceding C ABI call.
        assert_eq!(
            unsafe { slice::from_raw_parts(output.data, output.length) },
            value
        );
        // SAFETY: The output buffer came from the preceding C ABI call and is freed once.
        unsafe { thingdb_buffer_free(output) };
        // SAFETY: The handle is live and no longer used after this point.
        assert_eq!(unsafe { thingdb_close(db) }, ThingDbStatus::Ok);
    }

    #[test]
    fn c_abi_reports_invalid_arguments() {
        let mut db = std::ptr::null_mut();
        // SAFETY: `db` points to writable storage.
        assert_eq!(
            unsafe { thingdb_open(std::ptr::null(), 0, &raw mut db) },
            ThingDbStatus::InvalidArgument
        );
        // SAFETY: A null database is invalid and no memory is dereferenced.
        assert_eq!(
            unsafe { thingdb_close(std::ptr::null_mut()) },
            ThingDbStatus::InvalidArgument
        );
    }
}
