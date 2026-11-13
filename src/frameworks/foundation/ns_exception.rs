/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::ns_string::{from_rust_string, with_format};
use crate::dyld::{ConstantExports, FunctionExports, HostConstant};
use crate::mem::MutVoidPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, objc_exception_throw, release, retain,
    ClassExports, HostObject, NSZonePtr,
};
use crate::{export_c_func, Environment};

#[derive(Default)]
struct NSExceptionHostObject {
    name: id,
    reason: id,
    user_info: id,
}
impl HostObject for NSExceptionHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSException: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSExceptionHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)exceptionWithName:(id)name // NSString*
                 reason:(id)reason // NSString*
               userInfo:(id)user_info { // NSDictionary*
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithName:name reason:reason userInfo:user_info];
    autorelease(env, new)
}

+ (())raise:(id)name // NSString*
     format:(id)format, // NSString*
     ...args {
    let reason = with_format(env, format, args.start());
    let reason = from_rust_string(env, reason);
    let reason = autorelease(env, reason);
    let exception: id = msg_class![env; NSException exceptionWithName:name
                                                               reason:reason
                                                             userInfo:nil];
    objc_exception_throw(env, exception);
}

- (id)initWithName:(id)name // NSString*
            reason:(id)reason // NSString*
          userInfo:(id)user_info { // NSDictionary*
    let name: id = msg![env; name copy];
    let reason: id = msg![env; reason copy];
    retain(env, user_info);
    *env.objc.borrow_mut::<NSExceptionHostObject>(this) = NSExceptionHostObject {
        name,
        reason,
        user_info,
    };
    this
}

- (())dealloc {
    let &NSExceptionHostObject { name, reason, user_info } = env.objc.borrow(this);
    release(env, name);
    release(env, reason);
    release(env, user_info);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (())raise {
    objc_exception_throw(env, this);
}

- (id)name {
    env.objc.borrow::<NSExceptionHostObject>(this).name
}
- (id)reason {
    env.objc.borrow::<NSExceptionHostObject>(this).reason
}
- (id)userInfo {
    env.objc.borrow::<NSExceptionHostObject>(this).user_info
}
- (id)description {
    env.objc.borrow::<NSExceptionHostObject>(this).reason
}

@end

};

// All constants are NSExceptionName
pub const CONSTANTS: ConstantExports = &[
    (
        "_NSCharacterConversionException",
        HostConstant::NSString("NSCharacterConversionException"),
    ),
    (
        "_NSDecimalNumberDivideByZeroException",
        HostConstant::NSString("NSDecimalNumberDivideByZeroException"),
    ),
    (
        "_NSDecimalNumberExactnessException",
        HostConstant::NSString("NSDecimalNumberExactnessException"),
    ),
    (
        "_NSDecimalNumberOverflowException",
        HostConstant::NSString("NSDecimalNumberOverflowException"),
    ),
    (
        "_NSDecimalNumberUnderflowException",
        HostConstant::NSString("NSDecimalNumberUnderflowException"),
    ),
    (
        "_NSDestinationInvalidException",
        HostConstant::NSString("NSDestinationInvalidException"),
    ),
    (
        "_NSFileHandleOperationException",
        HostConstant::NSString("NSFileHandleOperationException"),
    ),
    (
        "_NSGenericException",
        HostConstant::NSString("NSGenericException"),
    ),
    (
        "_NSInternalInconsistencyException",
        HostConstant::NSString("NSInternalInconsistencyException"),
    ),
    (
        "_NSInvalidArchiveOperationException",
        HostConstant::NSString("NSInvalidArchiveOperationException"),
    ),
    (
        "_NSInvalidArgumentException",
        HostConstant::NSString("NSInvalidArgumentException"),
    ),
    (
        "_NSInvalidReceivePortException",
        HostConstant::NSString("NSInvalidReceivePortException"),
    ),
    (
        "_NSInvalidSendPortException",
        HostConstant::NSString("NSInvalidSendPortException"),
    ),
    (
        "_NSInvalidUnarchiveOperationException",
        HostConstant::NSString("NSInvalidUnarchiveOperationException"),
    ),
    (
        "_NSInvocationOperationCancelledException",
        HostConstant::NSString("NSInvocationOperationCancelledException"),
    ),
    (
        "_NSInvocationOperationVoidResultException",
        HostConstant::NSString("NSInvocationOperationVoidResultException"),
    ),
    (
        "_NSMallocException",
        HostConstant::NSString("NSMallocException"),
    ),
    (
        "_NSObjectInaccessibleException",
        HostConstant::NSString("NSObjectInaccessibleException"),
    ),
    (
        "_NSObjectNotAvailableException",
        HostConstant::NSString("NSObjectNotAvailableException"),
    ),
    (
        "_NSOldStyleException",
        HostConstant::NSString("NSOldStyleException"),
    ),
    (
        "_NSParseErrorException",
        HostConstant::NSString("NSParseErrorException"),
    ),
    (
        "_NSPortReceiveException",
        HostConstant::NSString("NSPortReceiveException"),
    ),
    (
        "_NSPortSendException",
        HostConstant::NSString("NSPortSendException"),
    ),
    (
        "_NSPortTimeoutException",
        HostConstant::NSString("NSPortTimeoutException"),
    ),
    (
        "_NSRangeException",
        HostConstant::NSString("NSRangeException"),
    ),
    (
        "_NSUndefinedKeyException",
        HostConstant::NSString("NSUndefinedKeyException"),
    ),
    (
        "_NSInconsistentArchiveException",
        HostConstant::NSString("NSInconsistentArchiveException"),
    ),
    (
        "_NSPPDIncludeNotFoundException",
        HostConstant::NSString("NSPPDIncludeNotFoundException"),
    ),
    (
        "_NSPPDIncludeStackOverflowException",
        HostConstant::NSString("NSPPDIncludeStackOverflowException"),
    ),
    (
        "_NSPPDIncludeStackUnderflowException",
        HostConstant::NSString("NSPPDIncludeStackUnderflowException"),
    ),
    (
        "_NSPPDParseException",
        HostConstant::NSString("NSPPDParseException"),
    ),
    (
        "_NSRTFPropertyStackOverflowException",
        HostConstant::NSString("NSRTFPropertyStackOverflowException"),
    ),
    (
        "_NSTIFFException",
        HostConstant::NSString("NSTIFFException"),
    ),
    (
        "_NSAbortModalException",
        HostConstant::NSString("NSAbortModalException"),
    ),
    (
        "_NSAbortPrintingException",
        HostConstant::NSString("NSAbortPrintingException"),
    ),
    (
        "_NSAccessibilityException",
        HostConstant::NSString("NSAccessibilityException"),
    ),
    (
        "_NSAppKitIgnoredException",
        HostConstant::NSString("NSAppKitIgnoredException"),
    ),
    (
        "_NSAppKitVirtualMemoryException",
        HostConstant::NSString("NSAppKitVirtualMemoryException"),
    ),
    (
        "_NSBadBitmapParametersException",
        HostConstant::NSString("NSBadBitmapParametersException"),
    ),
    (
        "_NSBadComparisonException",
        HostConstant::NSString("NSBadComparisonException"),
    ),
    (
        "_NSBadRTFColorTableException",
        HostConstant::NSString("NSBadRTFColorTableException"),
    ),
    (
        "_NSBadRTFDirectiveException",
        HostConstant::NSString("NSBadRTFDirectiveException"),
    ),
    (
        "_NSBadRTFFontTableException",
        HostConstant::NSString("NSBadRTFFontTableException"),
    ),
    (
        "_NSBadRTFStyleSheetException",
        HostConstant::NSString("NSBadRTFStyleSheetException"),
    ),
    (
        "_NSBrowserIllegalDelegateException",
        HostConstant::NSString("NSBrowserIllegalDelegateException"),
    ),
    (
        "_NSColorListIOException",
        HostConstant::NSString("NSColorListIOException"),
    ),
    (
        "_NSColorListNotEditableException",
        HostConstant::NSString("NSColorListNotEditableException"),
    ),
    (
        "_NSDraggingException",
        HostConstant::NSString("NSDraggingException"),
    ),
    (
        "_NSFontUnavailableException",
        HostConstant::NSString("NSFontUnavailableException"),
    ),
    (
        "_NSIllegalSelectorException",
        HostConstant::NSString("NSIllegalSelectorException"),
    ),
    (
        "_NSImageCacheException",
        HostConstant::NSString("NSImageCacheException"),
    ),
    (
        "_NSNibLoadingException",
        HostConstant::NSString("NSNibLoadingException"),
    ),
    (
        "_NSPasteboardCommunicationException",
        HostConstant::NSString("NSPasteboardCommunicationException"),
    ),
    (
        "_NSPrintOperationExistsException",
        HostConstant::NSString("NSPrintOperationExistsException"),
    ),
    (
        "_NSPrintPackageException",
        HostConstant::NSString("NSPrintPackageException"),
    ),
    (
        "_NSPrintingCommunicationException",
        HostConstant::NSString("NSPrintingCommunicationException"),
    ),
    (
        "_NSTextLineTooLongException",
        HostConstant::NSString("NSTextLineTooLongException"),
    ),
    (
        "_NSTextNoSelectionException",
        HostConstant::NSString("NSTextNoSelectionException"),
    ),
    (
        "_NSTextReadException",
        HostConstant::NSString("NSTextReadException"),
    ),
    (
        "_NSTextWriteException",
        HostConstant::NSString("NSTextWriteException"),
    ),
    (
        "_NSTypedStreamVersionException",
        HostConstant::NSString("NSTypedStreamVersionException"),
    ),
    (
        "_NSWindowServerCommunicationException",
        HostConstant::NSString("NSWindowServerCommunicationException"),
    ),
    (
        "_NSWordTablesReadException",
        HostConstant::NSString("NSWordTablesReadException"),
    ),
    (
        "_NSWordTablesWriteException",
        HostConstant::NSString("NSWordTablesWriteException"),
    ),
    (
        "_UIViewControllerHierarchyInconsistencyException",
        HostConstant::NSString("UIViewControllerHierarchyInconsistencyException"),
    ),
    (
        "_UIApplicationInvalidInterfaceOrientationException",
        HostConstant::NSString("UIApplicationInvalidInterfaceOrientationException"),
    ),
];

/// This exception handler is supposed to do last-minute logging before the
/// program terminates. For our purposes, it's completely safe to ignore that.
fn NSSetUncaughtExceptionHandler(_env: &mut Environment, handler: MutVoidPtr) {
    log!(
        "TODO: Ignoring uncaught exception handler at address {:?}",
        handler
    );
}

pub const FUNCTIONS: FunctionExports = &[export_c_func!(NSSetUncaughtExceptionHandler(_))];
