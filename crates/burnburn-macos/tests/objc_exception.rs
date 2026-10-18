//! ADR 0002 / IM20: an Objective-C exception raised by an AppKit/Foundation call becomes a Rust
//! error at the call site (`objc2::exception::catch`) instead of aborting the process.

#[cfg(target_os = "macos")]
#[test]
#[allow(unsafe_code)]
fn objc_exception_becomes_an_error_instead_of_an_abort() {
    use burnburn_macos::appkit::objc_try;
    use objc2::msg_send;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2_foundation::NSArray;

    let empty = NSArray::<NSObject>::new();
    let result = objc_try("objectAtIndex: out of range", || {
        // SAFETY: a valid message to a valid array; the out-of-range index raises NSRangeException,
        // which is exactly what objc_try must contain.
        let _: *mut AnyObject = unsafe { msg_send![&empty, objectAtIndex: 3usize] };
    });
    let error = result.expect_err("the exception must become an error");
    assert!(error.contains("NSRangeException"), "{error}");
    assert!(error.contains("objectAtIndex: out of range"), "names the call: {error}");
    assert_eq!(objc_try("count", || empty.count()), Ok(0));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn objc_exception_becomes_an_error_instead_of_an_abort() {
    println!("SKIPPED (not macOS; not a pass)");
}
