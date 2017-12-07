// Copyright 2017 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement.  This, along with the Licenses can be
// found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

// #![allow(unsafe_code)]

// use App;
// use errors::AppError;
// use ffi_utils::catch_unwind_error_code;
// use safe_core::ffi::ipc::req::ContainerPermissions;
// use safe_core::ipc::req::containers_from_repr_c;
// use test_utils::{create_app, create_app_with_access};


// /// Creates a random app instance for testing.
// #[no_mangle]
// #[cfg_attr(feature = "cargo-clippy", allow(not_unsafe_ptr_arg_deref))]
// pub extern "C" fn test_create_app(o_app: *mut *mut App) -> i32 {
//     catch_unwind_error_code(|| -> Result<(), AppError> {
//         let app = create_app();
//         unsafe {
//             *o_app = Box::into_raw(Box::new(app));
//         }
//         Ok(())
//     })
// }

// /// Create a random app instance for testing, with access to containers.
// #[no_mangle]
// #[cfg_attr(feature = "cargo-clippy", allow(not_unsafe_ptr_arg_deref))]
// pub extern "C" fn test_create_app_with_access(
//     access_info_ptr: *const ContainerPermissions,
//     access_info_len: usize,
//     o_app: *mut *mut App,
// ) -> i32 {
//     catch_unwind_error_code(|| -> Result<(), AppError> {
//         let containers = unsafe { containers_from_repr_c(access_info_ptr, access_info_len)? };
//         let app = create_app_with_access(containers);
//         unsafe {
//             *o_app = Box::into_raw(Box::new(app));
//         }
//         Ok(())
//     })
// }
