// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net Commercial License,
// version 1.0 or later, or (2) The General Public License (GPL), version 3, depending on which
// licence you accepted on initial access to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project generally, you agree to be
// bound by the terms of the MaidSafe Contributor Agreement, version 1.0.  This, along with the
// Licenses can be found in the root directory of this project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network Software distributed
// under the GPL Licence is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.
//
// Please review the Licences for the specific language governing permissions and limitations
// relating to use of the SAFE Network Software.

use core::errors::CoreError;
use ffi::app::App;
use ffi::errors::FfiError;
use ffi::helper;
use ffi::low_level_api::{AppendableDataHandle, DataIdHandle, EncryptKeyHandle, SignKeyHandle};
use ffi::low_level_api::object_cache::object_cache;
use routing::{AppendWrapper, AppendedData, Data, Filter, PrivAppendableData, PrivAppendedData,
              PubAppendableData, XOR_NAME_LEN, XorName};
use std::collections::BTreeSet;
use std::iter;
use std::ptr;

/// Wrapper for PrivAppendableData and PubAppendableData.
#[derive(Clone)]
pub enum AppendableData {
    /// Public appendable data.
    Pub(PubAppendableData),
    /// Private appendable data.
    Priv(PrivAppendableData),
}

impl AppendableData {
    fn filter_mut(&mut self) -> &mut Filter {
        match *self {
            AppendableData::Pub(ref mut data) => &mut data.filter,
            AppendableData::Priv(ref mut data) => &mut data.filter,
        }
    }
}

impl Into<Data> for AppendableData {
    fn into(self) -> Data {
        match self {
            AppendableData::Pub(data) => Data::PubAppendable(data),
            AppendableData::Priv(data) => Data::PrivAppendable(data),
        }
    }
}

/// Filter Type
#[repr(C)]
pub enum FilterType {
    /// BlackList
    BlackList,
    /// WhiteList
    WhiteList,
}

/// Create new PubAppendableData
#[no_mangle]
pub unsafe extern "C" fn appendable_data_new_pub(app: *const App,
                                                 name: *const [u8; XOR_NAME_LEN],
                                                 o_handle: *mut AppendableDataHandle)
                                                 -> i32 {
    helper::catch_unwind_i32(|| {
        let client = (*app).get_client();
        let name = XorName(*name);

        let (owner_key, sign_key) = {
            let client = unwrap!(client.lock());
            let owner_key = *ffi_try!(client.get_public_signing_key());
            let sign_key = ffi_try!(client.get_secret_signing_key()).clone();
            (owner_key, sign_key)
        };

        let data = PubAppendableData::new(name,
                                          0,
                                          vec![owner_key],
                                          Default::default(),
                                          Default::default(),
                                          Filter::black_list(iter::empty()),
                                          Some(&sign_key));
        let data = AppendableData::Pub(ffi_try!(data.map_err(CoreError::from)));
        let handle = unwrap!(object_cache().lock()).insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// Create new PrivAppendableData
#[no_mangle]
pub unsafe extern "C" fn appendable_data_new_priv(app: *const App,
                                                  name: *const [u8; XOR_NAME_LEN],
                                                  o_handle: *mut AppendableDataHandle)
                                                  -> i32 {
    helper::catch_unwind_i32(|| {
        let app = &*app;
        let client = app.get_client();
        let name = XorName(*name);

        let (owner_key, sign_key) = {
            let client = unwrap!(client.lock());
            let owner_key = *ffi_try!(client.get_public_signing_key());
            let sign_key = ffi_try!(client.get_secret_signing_key()).clone();
            (owner_key, sign_key)
        };

        let data = PrivAppendableData::new(name,
                                           0,
                                           vec![owner_key],
                                           Default::default(),
                                           Default::default(),
                                           Filter::black_list(iter::empty()),
                                           ffi_try!(app.asym_keys()).0,
                                           Some(&sign_key));
        let data = AppendableData::Priv(ffi_try!(data.map_err(CoreError::from)));
        let handle = unwrap!(object_cache().lock()).insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// Get existing appendable data from Network.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_get(app: *const App,
                                             data_id_h: DataIdHandle,
                                             o_handle: *mut AppendableDataHandle)
                                             -> i32 {
    helper::catch_unwind_i32(|| {
        let data_id = *ffi_try!(unwrap!(object_cache().lock()).get_data_id(data_id_h));

        let client = (*app).get_client();
        let resp_getter = ffi_try!(unwrap!(client.lock()).get(data_id, None));
        let data = match ffi_try!(resp_getter.get()) {
            Data::PubAppendable(data) => AppendableData::Pub(data),
            Data::PrivAppendable(data) => AppendableData::Priv(data),
            _ => ffi_try!(Err(CoreError::ReceivedUnexpectedData)),
        };

        let handle = unwrap!(object_cache().lock()).insert_appendable_data(data);

        ptr::write(o_handle, handle);
        0
    })
}

/// Extract DataIdentifier from AppendableData.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_extract_data_id(ad_h: AppendableDataHandle,
                                                         o_handle: *mut DataIdHandle)
                                                         -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let data_id = match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
            AppendableData::Pub(ref elt) => elt.identifier(),
            AppendableData::Priv(ref elt) => elt.identifier(),
        };
        let handle = object_cache.new_handle();
        if let Some(prev) = object_cache.data_id.insert(handle, data_id) {
            debug!("Displaced DataIdentifier from ObjectCache: {:?}", prev);
        }

        ptr::write(o_handle, handle);
        0
    })
}

/// PUT appendable data.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_put(app: *const App, ad_h: AppendableDataHandle) -> i32 {
    helper::catch_unwind_i32(|| {
        let data = {
            let mut object_cache = unwrap!(object_cache().lock());
            ffi_try!(object_cache.get_appendable_data(ad_h)).clone()
        };

        let client = (*app).get_client();
        let resp_getter = ffi_try!(unwrap!(client.lock()).put(data.into(), None));
        ffi_try!(resp_getter.get());

        0
    })
}

/// POST appendable data (bumps the version).
#[no_mangle]
pub unsafe extern "C" fn appendable_data_post(app: *const App, ad_h: AppendableDataHandle) -> i32 {
    helper::catch_unwind_i32(|| {
        let client = (*app).get_client();

        let new_ad = {
            let sign_key = ffi_try!(unwrap!(client.lock()).get_secret_signing_key()).clone();
            let mut object_cache = unwrap!(object_cache().lock());
            let ad = ffi_try!(object_cache.get_appendable_data(ad_h));

            match *ad {
                AppendableData::Pub(ref old_data) => {
                    let new_data =
                        ffi_try!(PubAppendableData::new(old_data.name,
                                                        old_data.version + 1,
                                                        old_data.current_owner_keys.clone(),
                                                        old_data.previous_owner_keys.clone(),
                                                        old_data.deleted_data.clone(),
                                                        old_data.filter.clone(),
                                                        Some(&sign_key))
                            .map_err(CoreError::from));
                    AppendableData::Pub(new_data)
                }
                AppendableData::Priv(ref old_data) => {
                    let new_data =
                        ffi_try!(PrivAppendableData::new(old_data.name,
                                                         old_data.version + 1,
                                                         old_data.current_owner_keys.clone(),
                                                         old_data.previous_owner_keys.clone(),
                                                         old_data.deleted_data.clone(),
                                                         old_data.filter.clone(),
                                                         old_data.encrypt_key.clone(),
                                                         Some(&sign_key))
                            .map_err(CoreError::from));
                    AppendableData::Priv(new_data)
                }
            }
        };
        let resp_getter = ffi_try!(unwrap!(client.lock()).post(new_ad.clone().into(), None));
        ffi_try!(resp_getter.get());
        let _ = unwrap!(object_cache().lock()).appendable_data.insert(ad_h, new_ad);

        0
    })
}

// TODO: DELETE (disabled for now)

/// Get the filter type
#[no_mangle]
pub unsafe extern "C" fn appendable_data_filter_type(ad_h: AppendableDataHandle,
                                                     o_type: *mut FilterType)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let ad = ffi_try!(object_cache.get_appendable_data(ad_h));
        let filter = ad.filter_mut();
        let filter_type = match *filter {
            Filter::BlackList(_) => FilterType::BlackList,
            Filter::WhiteList(_) => FilterType::WhiteList,
        };

        ptr::write(o_type, filter_type);
        0
    })
}

/// Switch the filter of the appendable data.
#[no_mangle]
pub extern "C" fn appendable_data_toggle_filter(ad_h: AppendableDataHandle) -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let ad = ffi_try!(object_cache.get_appendable_data(ad_h));

        let filter = ad.filter_mut();
        match *filter {
            Filter::BlackList(_) => *filter = Filter::white_list(iter::empty()),
            Filter::WhiteList(_) => *filter = Filter::black_list(iter::empty()),
        }

        0
    })
}

/// Insert a new entry to the (whitelist or blacklist) filter. If the key was
/// already present in the filter, this is a no-op.
#[no_mangle]
pub extern "C" fn appendable_data_insert_to_filter(ad_h: AppendableDataHandle,
                                                   sign_key_h: SignKeyHandle)
                                                   -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let sign_key = *ffi_try!(object_cache.get_sign_key(sign_key_h));
        let ad = ffi_try!(object_cache.get_appendable_data(ad_h));

        let _ = match *ad.filter_mut() {
            Filter::WhiteList(ref mut list) |
            Filter::BlackList(ref mut list) => list.insert(sign_key),
        };

        0
    })
}

/// Remove the given key from the (whitelist or blacklist) filter. If the key
/// isn't present in the filter, this is a no-op.
#[no_mangle]
pub extern "C" fn appendable_data_remove_from_filter(ad_h: AppendableDataHandle,
                                                     sign_key_h: SignKeyHandle)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let sign_key = *ffi_try!(object_cache.get_sign_key(sign_key_h));
        let ad = ffi_try!(object_cache.get_appendable_data(ad_h));

        let _ = match *ad.filter_mut() {
            Filter::WhiteList(ref mut list) |
            Filter::BlackList(ref mut list) => list.remove(&sign_key),
        };

        0
    })
}

/// Get the owner's encrypt key
#[no_mangle]
pub unsafe extern "C" fn appendable_data_encrypt_key(ad_h: AppendableDataHandle,
                                                     o_handle: *mut EncryptKeyHandle)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let pk = match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
            AppendableData::Priv(ref elt) => elt.encrypt_key.clone(),
            _ => ffi_try!(Err(FfiError::UnsupportedOperation)),
        };
        let handle = object_cache.new_handle();
        if let Some(prev) = object_cache.encrypt_key.insert(handle, pk) {
            debug!("Displaced Public Encrypt Key from ObjectCache: {:?}", prev);
        }

        ptr::write(o_handle, handle);
        0
    })
}

/// Get number of appended data items.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_num_of_data(ad_h: AppendableDataHandle,
                                                     o_num: *mut u64)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        let ad = ffi_try!(object_cache.get_appendable_data(ad_h));
        let num = match *ad {
            AppendableData::Pub(ref elt) => elt.data.len(),
            AppendableData::Priv(ref elt) => elt.data.len(),
        };

        ptr::write(o_num, num as u64);
        0
    })
}

/// Get nth appended DataIdentifier.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_nth_data_id(app: *const App,
                                                     ad_h: AppendableDataHandle,
                                                     n: u64,
                                                     o_handle: *mut DataIdHandle)
                                                     -> i32 {
    helper::catch_unwind_i32(|| {
        let app = &*app;

        let mut object_cache = unwrap!(object_cache().lock());

        let data_id = match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
            AppendableData::Priv(ref elt) => {
                let priv_data = ffi_try!(nth(&elt.data, n));
                let &(ref pk, ref sk) = ffi_try!(app.asym_keys());
                ffi_try!(priv_data.open(pk, sk).map_err(CoreError::from)).pointer

            }
            AppendableData::Pub(ref elt) => ffi_try!(nth(&elt.data, n)).pointer,
        };

        let handle = object_cache.new_handle();
        if let Some(prev) = object_cache.data_id.insert(handle, data_id) {
            debug!("Displaced DataIdentifier from ObjectCache: {:?}", prev);
        }

        ptr::write(o_handle, handle);
        0
    })
}

/// Get nth sign key.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_nth_sign_key(app: *const App,
                                                      ad_h: AppendableDataHandle,
                                                      n: u64,
                                                      o_handle: *mut SignKeyHandle)
                                                      -> i32 {
    helper::catch_unwind_i32(|| {
        let app = &*app;

        let mut object_cache = unwrap!(object_cache().lock());

        let sign_key = match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
            AppendableData::Priv(ref elt) => {
                let priv_data = ffi_try!(nth(&elt.data, n));
                let &(ref pk, ref sk) = ffi_try!(app.asym_keys());
                ffi_try!(priv_data.open(pk, sk).map_err(CoreError::from)).sign_key

            }
            AppendableData::Pub(ref elt) => ffi_try!(nth(&elt.data, n)).sign_key,
        };

        let handle = object_cache.new_handle();
        if let Some(prev) = object_cache.sign_key.insert(handle, sign_key) {
            debug!("Displaced Public Sign Key from ObjectCache: {:?}", prev);
        }

        ptr::write(o_handle, handle);
        0
    })
}

/// Append data.
#[no_mangle]
pub unsafe extern "C" fn appendable_data_append(app: *const App,
                                                ad_h: AppendableDataHandle,
                                                data_id_h: DataIdHandle)
                                                -> i32 {
    helper::catch_unwind_i32(|| {
        let client = (*app).get_client();

        let append_wrapper = {
            let mut object_cache = unwrap!(object_cache().lock());
            let data_id = *ffi_try!(object_cache.get_data_id(data_id_h));

            let client = unwrap!(client.lock());
            let sign_pk = ffi_try!(client.get_public_signing_key());
            let sign_sk = ffi_try!(client.get_secret_signing_key());

            let appended_data = ffi_try!(AppendedData::new(data_id, *sign_pk, sign_sk)
                .map_err(CoreError::from));

            match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
                AppendableData::Priv(ref elt) => {
                    let priv_appended_data = ffi_try!(PrivAppendedData::new(&appended_data,
                                                                            &elt.encrypt_key)
                        .map_err(CoreError::from));
                    ffi_try!(AppendWrapper::new_priv(elt.name,
                                                     priv_appended_data,
                                                     (sign_pk, sign_sk),
                                                     elt.version)
                        .map_err(CoreError::from))
                }
                AppendableData::Pub(ref elt) => {
                    AppendWrapper::new_pub(elt.name, appended_data, elt.version)
                }
            }
        };

        let resp_getter = ffi_try!(unwrap!(client.lock()).append(append_wrapper, None));
        ffi_try!(resp_getter.get());

        0
    })
}

/// Remove the n-th data item from the appendable data. The data has to be
/// POST'd afterwards for the change to be registered by the network.
#[no_mangle]
pub extern "C" fn appendable_data_remove_nth(ad_h: AppendableDataHandle, n: u64) -> i32 {
    helper::catch_unwind_i32(|| {
        let mut object_cache = unwrap!(object_cache().lock());
        match *ffi_try!(object_cache.get_appendable_data(ad_h)) {
            AppendableData::Pub(ref mut data) => {
                let item = ffi_try!(nth(&data.data, n)).clone();
                data.delete(item);
            }
            AppendableData::Priv(ref mut data) => {
                let item = ffi_try!(nth(&data.data, n)).clone();
                data.delete(item);
            }
        }

        0
    })
}

/// Free AppendableData handle
#[no_mangle]
pub extern "C" fn appendable_data_free(handle: AppendableDataHandle) -> i32 {
    helper::catch_unwind_i32(|| {
        let _ = ffi_try!(unwrap!(object_cache().lock())
            .appendable_data
            .remove(&handle)
            .ok_or(FfiError::InvalidAppendableDataHandle));

        0
    })
}

// Convenience function to access n-th item from the given set, returning FfiError::InvalidIndex
// if not found.
fn nth<T>(items: &BTreeSet<T>, n: u64) -> Result<&T, FfiError> {
    items.iter().nth(n as usize).ok_or(FfiError::InvalidIndex)
}
