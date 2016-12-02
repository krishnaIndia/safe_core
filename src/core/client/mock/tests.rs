// Copyright 2016 MaidSafe.net limited.
//
// This SAFE Network Software is licensed to you under (1) the MaidSafe.net
// Commercial License, version 1.0 or later, or (2) The General Public License
// (GPL), version 3, depending on which licence you accepted on initial access
// to the Software (the "Licences").
//
// By contributing code to the SAFE Network Software, or to this project
// generally, you agree to be bound by the terms of the MaidSafe Contributor
// Agreement, version 1.0.
// This, along with the Licenses can be found in the root directory of this
// project at LICENSE, COPYING and CONTRIBUTOR.
//
// Unless required by applicable law or agreed to in writing, the SAFE Network
// Software distributed under the GPL Licence is distributed on an "AS IS"
// BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
// implied.
//
// Please review the Licences for the specific language governing permissions
// and limitations relating to use of the SAFE Network Software.

use core::utility;
use rand;
use routing::{AccountInfo, Action, Authority, ClientError, EntryAction, Event, FullId,
              ImmutableData, MessageId, MutableData, PermissionSet, Response,
              TYPE_TAG_SESSION_PACKET, User, Value, XorName};
use rust_sodium::crypto::hash::sha256;
use rust_sodium::crypto::sign;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;
use super::routing::Routing;
use super::vault::DEFAULT_MAX_MUTATIONS;

// Helper macro to receive a routing event and assert it's a response
// success.
macro_rules! expect_success {
        ($rx:expr, $msg_id:expr, $res:path) => {
            match unwrap!($rx.recv_timeout(Duration::from_secs(10))) {
                Event::Response {
                    response: $res { res, msg_id, }, ..
                } => {
                    assert_eq!(msg_id, $msg_id);

                    match res {
                        Ok(value) => value,
                        Err(err) => panic!("Unexpected error {:?}", err),
                    }
                }
                event => panic!("Unexpected event {:?}", event),
            }
        }
    }

// Helper macro to receive a routing event and assert it's a response
// failure.
macro_rules! expect_failure {
        ($rx:expr, $msg_id:expr, $res:path, $err:pat) => {
            match unwrap!($rx.recv_timeout(Duration::from_secs(10))) {
                Event::Response {
                    response: $res { res, msg_id, }, ..
                } => {
                    assert_eq!(msg_id, $msg_id);

                    match res {
                        Ok(_) => panic!("Unexpected success"),
                        Err($err) => (),
                        Err(err) => panic!("Unexpected error {:?}", err),
                    }
                }
                event => panic!("Unexpected event {:?}", event),
            }
        }
    }

#[test]
fn immutable_data_basics() {
    let (routing, routing_rx, full_id) = setup();

    // Create account
    let owner_key = *full_id.public_id().signing_public_key();
    let client_mgr = create_account(&routing, &routing_rx, owner_key);

    // Construct ImmutableData
    let orig_data = ImmutableData::new(unwrap!(utility::generate_random_vector(100)));
    let nae_mgr = Authority::NaeManager(*orig_data.name());

    // GetIData should fail
    let msg_id = MessageId::new();
    unwrap!(routing.get_idata(nae_mgr.clone(), *orig_data.name(), msg_id));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::GetIData,
                    ClientError::NoSuchData);

    // First PutIData should succeed
    let msg_id = MessageId::new();
    unwrap!(routing.put_idata(client_mgr.clone(), orig_data.clone(), msg_id));
    expect_success!(routing_rx, msg_id, Response::PutIData);

    // Now GetIData should pass
    let msg_id = MessageId::new();
    unwrap!(routing.get_idata(nae_mgr.clone(), *orig_data.name(), msg_id));
    let got_data = expect_success!(routing_rx, msg_id, Response::GetIData);
    assert_eq!(got_data, orig_data);

    // GetAccountInfo should pass and show one mutation performed
    let account_info = do_get_account_info(&routing, &routing_rx, client_mgr);
    assert_eq!(account_info.mutations_done, 1);
    assert_eq!(account_info.mutations_available, DEFAULT_MAX_MUTATIONS - 1);

    // Subsequent PutIData for same data should succeed - De-duplication
    let msg_id = MessageId::new();
    unwrap!(routing.put_idata(client_mgr.clone(), orig_data.clone(), msg_id));
    expect_success!(routing_rx, msg_id, Response::PutIData);

    // GetIData should succeed
    let msg_id = MessageId::new();
    unwrap!(routing.get_idata(nae_mgr.clone(), *orig_data.name(), msg_id));
    let got_data = expect_success!(routing_rx, msg_id, Response::GetIData);
    assert_eq!(got_data, orig_data);

    // GetAccountInfo should pass and show two mutations performed
    let account_info = do_get_account_info(&routing, &routing_rx, client_mgr);
    assert_eq!(account_info.mutations_done, 2);
    assert_eq!(account_info.mutations_available, DEFAULT_MAX_MUTATIONS - 2);
}

#[test]
fn mutable_data_basics() {
    let (routing, routing_rx, full_id) = setup();

    // Create account
    let owner_key = *full_id.public_id().signing_public_key();
    let client_mgr = create_account(&routing, &routing_rx, owner_key);

    // Construct MutableData
    let name = rand::random();
    let tag = 1000u64;

    let data = unwrap!(MutableData::new(name,
                                        tag,
                                        Default::default(),
                                        Default::default(),
                                        btree_set!(owner_key)));
    let nae_mgr = Authority::NaeManager(*data.name());

    // Operations on non-existing MutableData should fail.
    let msg_id = MessageId::new();
    unwrap!(routing.get_mdata_version(nae_mgr.clone(), name, tag, msg_id));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::GetMDataVersion,
                    ClientError::NoSuchData);

    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_entries(nae_mgr.clone(), name, tag, msg_id));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::ListMDataEntries,
                    ClientError::NoSuchData);

    // PutMData
    let msg_id = MessageId::new();
    unwrap!(routing.put_mdata(client_mgr, data, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::PutMData);

    // GetMDataVersion should respond with 0
    let msg_id = MessageId::new();
    unwrap!(routing.get_mdata_version(nae_mgr, name, tag, msg_id));
    let version = expect_success!(routing_rx, msg_id, Response::GetMDataVersion);
    assert_eq!(version, 0);

    // ListMDataEntries, ListMDataKeys and ListMDataValues should all respond
    // with empty collections.
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_entries(nae_mgr, name, tag, msg_id));
    let entries = expect_success!(routing_rx, msg_id, Response::ListMDataEntries);
    assert!(entries.is_empty());

    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_keys(nae_mgr, name, tag, msg_id));
    let keys = expect_success!(routing_rx, msg_id, Response::ListMDataKeys);
    assert!(keys.is_empty());

    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_values(nae_mgr, name, tag, msg_id));
    let values = expect_success!(routing_rx, msg_id, Response::ListMDataValues);
    assert!(values.is_empty());

    // Add couple of entries
    let key0 = b"key0";
    let key1 = b"key1";
    let value0_v0 = unwrap!(utility::generate_random_vector(10));
    let value1_v0 = unwrap!(utility::generate_random_vector(10));

    let actions = btree_map![
            key0.to_vec() => EntryAction::Ins(Value {
                content: value0_v0.clone(),
                entry_version: 0,
            }),
            key1.to_vec() => EntryAction::Ins(Value {
                content: value1_v0.clone(),
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // ListMDataEntries
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_entries(nae_mgr, name, tag, msg_id));
    let entries = expect_success!(routing_rx, msg_id, Response::ListMDataEntries);
    assert_eq!(entries.len(), 2);

    let entry = unwrap!(entries.get(&key0[..]));
    assert_eq!(entry.content, value0_v0);
    assert_eq!(entry.entry_version, 0);

    let entry = unwrap!(entries.get(&key1[..]));
    assert_eq!(entry.content, value1_v0);
    assert_eq!(entry.entry_version, 0);

    // ListMDataKeys
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_keys(nae_mgr, name, tag, msg_id));
    let keys = expect_success!(routing_rx, msg_id, Response::ListMDataKeys);
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&key0[..]));
    assert!(keys.contains(&key1[..]));

    // ListMDataValues
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_values(nae_mgr, name, tag, msg_id));
    let values = expect_success!(routing_rx, msg_id, Response::ListMDataValues);
    assert_eq!(values.len(), 2);

    // GetMDataValue with existing key
    let msg_id = MessageId::new();
    unwrap!(routing.get_mdata_value(nae_mgr, name, tag, key0.to_vec(), msg_id));
    let value = expect_success!(routing_rx, msg_id, Response::GetMDataValue);
    assert_eq!(value.content, value0_v0);
    assert_eq!(value.entry_version, 0);

    // GetMDataValue with non-existing key
    let key2 = b"key2";
    let msg_id = MessageId::new();
    unwrap!(routing.get_mdata_value(nae_mgr, name, tag, key2.to_vec(), msg_id));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::GetMDataValue,
                    ClientError::NoSuchEntry);

    // Mutate the entries: insert, update and delete
    let value0_v1 = unwrap!(utility::generate_random_vector(10));
    let value2_v0 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key0.to_vec() => EntryAction::Update(Value {
                content: value0_v1.clone(),
                entry_version: 1,
            }),
            key1.to_vec() => EntryAction::Del(1),
            key2.to_vec() => EntryAction::Ins(Value {
                content: value2_v0.clone(),
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // ListMDataEntries should respond with modified entries
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_entries(nae_mgr, name, tag, msg_id));
    let entries = expect_success!(routing_rx, msg_id, Response::ListMDataEntries);
    assert_eq!(entries.len(), 3);

    // Updated entry
    let entry = unwrap!(entries.get(&key0[..]));
    assert_eq!(entry.content, value0_v1);
    assert_eq!(entry.entry_version, 1);

    // Deleted entry
    let entry = unwrap!(entries.get(&key1[..]));
    assert!(entry.content.is_empty());
    assert_eq!(entry.entry_version, 1);

    // Inserted entry
    let entry = unwrap!(entries.get(&key2[..]));
    assert_eq!(entry.content, value2_v0);
    assert_eq!(entry.entry_version, 0);
}

#[test]
fn mutable_data_entry_versioning() {
    let (routing, routing_rx, full_id) = setup();

    let owner_key = *full_id.public_id().signing_public_key();
    let client_mgr = create_account(&routing, &routing_rx, owner_key);

    // Construct MutableData
    let name = rand::random();
    let tag = 1000u64;

    let data = unwrap!(MutableData::new(name,
                                        tag,
                                        Default::default(),
                                        Default::default(),
                                        btree_set!(owner_key)));

    // PutMData
    let msg_id = MessageId::new();
    unwrap!(routing.put_mdata(client_mgr, data, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::PutMData);

    // Insert a new entry
    let key = b"key0";
    let value_v0 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key.to_vec() => EntryAction::Ins(Value {
                content: value_v0,
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // Attempt to update it without version bump fails.
    let value_v1 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key.to_vec() => EntryAction::Update(Value {
                content: value_v1.clone(),
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::InvalidSuccessor);

    // Attempt to update it with incorrect version fails.
    let actions = btree_map![
            key.to_vec() => EntryAction::Update(Value {
                content: value_v1.clone(),
                entry_version: 314159265,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::InvalidSuccessor);

    // Update with correct version bump succeeds.
    let actions = btree_map![
            key.to_vec() => EntryAction::Update(Value {
                content: value_v1.clone(),
                entry_version: 1,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // Delete without version bump fails.
    let actions = btree_map![
            key.to_vec() => EntryAction::Del(1)
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::InvalidSuccessor);

    // Delete with correct version bump succeeds.
    let actions = btree_map![
            key.to_vec() => EntryAction::Del(2)
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);
}

#[test]
fn mutable_data_permissions() {
    let (routing, routing_rx, full_id) = setup();

    let owner_key = *full_id.public_id().signing_public_key();
    let other_key = sign::gen_keypair().0;

    let client_mgr = create_account(&routing, &routing_rx, owner_key);

    // Construct MutableData with some entries and empty permissions.
    let name = rand::random();
    let tag = 1000u64;

    let key0 = b"key0";
    let value0_v0 = unwrap!(utility::generate_random_vector(10));

    let entries = btree_map![
            key0.to_vec() => Value { content: value0_v0, entry_version: 0 }
        ];


    let data = unwrap!(MutableData::new(name,
                                        tag,
                                        Default::default(),
                                        entries,
                                        btree_set!(owner_key)));

    let nae_mgr = Authority::NaeManager(*data.name());

    // Put it to the network.
    let msg_id = MessageId::new();
    unwrap!(routing.put_mdata(client_mgr, data, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::PutMData);

    // ListMDataPermissions responds with empty collection.
    let msg_id = MessageId::new();
    unwrap!(routing.list_mdata_permissions(nae_mgr, name, tag, msg_id));
    let permissions = expect_success!(routing_rx, msg_id, Response::ListMDataPermissions);
    assert!(permissions.is_empty());

    // Owner can do anything by default.
    let value0_v1 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key0.to_vec() => EntryAction::Update(Value {
                content: value0_v1,
                entry_version: 1,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, owner_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // Other users can't mutate any entry, by default.
    let value0_v2 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key0.to_vec() => EntryAction::Update(Value {
                content: value0_v2.clone(),
                entry_version: 2,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, other_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::AccessDenied);


    // Grant insert permission for `other_key`.
    let mut perms = PermissionSet::new();
    let _ = perms.allow(Action::Insert);

    let msg_id = MessageId::new();
    unwrap!(routing.set_mdata_user_permissions(client_mgr,
                                               name,
                                               tag,
                                               User::Key(other_key),
                                               perms,
                                               1,
                                               msg_id,
                                               owner_key));
    expect_success!(routing_rx, msg_id, Response::SetMDataUserPermissions);

    // The version is bumped.
    let msg_id = MessageId::new();
    unwrap!(routing.get_mdata_version(nae_mgr, name, tag, msg_id));
    let version = expect_success!(routing_rx, msg_id, Response::GetMDataVersion);
    assert_eq!(version, 1);

    // `other_key` still can't update entries.
    let actions = btree_map![
            key0.to_vec() => EntryAction::Update(Value {
                content: value0_v2.clone(),
                entry_version: 2,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, other_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::AccessDenied);

    // But they can insert new ones.
    let key1 = b"key1";
    let value1_v0 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key1.to_vec() => EntryAction::Ins(Value {
                content: value1_v0,
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, other_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // Attempt to modify permissions without proper version bump fails
    let mut perms = PermissionSet::new();
    let _ = perms.allow(Action::Insert).allow(Action::Update);

    let msg_id = MessageId::new();
    unwrap!(routing.set_mdata_user_permissions(client_mgr,
                                               name,
                                               tag,
                                               User::Key(other_key),
                                               perms,
                                               1,
                                               msg_id,
                                               owner_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::SetMDataUserPermissions,
                    ClientError::InvalidSuccessor);

    // Modifing permissions with version bump succeeds.
    let mut perms = PermissionSet::new();
    let _ = perms.allow(Action::Insert).allow(Action::Update);

    let msg_id = MessageId::new();
    unwrap!(routing.set_mdata_user_permissions(client_mgr,
                                               name,
                                               tag,
                                               User::Key(other_key),
                                               perms,
                                               2,
                                               msg_id,
                                               owner_key));
    expect_success!(routing_rx, msg_id, Response::SetMDataUserPermissions);

    // `other_key` can now update entries.
    let actions = btree_map![
            key0.to_vec() => EntryAction::Update(Value {
                content: value0_v2,
                entry_version: 2,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, other_key));
    expect_success!(routing_rx, msg_id, Response::MutateMDataEntries);

    // Revoke all permissions from `other_key`.
    let msg_id = MessageId::new();
    unwrap!(routing.del_mdata_user_permissions(client_mgr,
                                               name,
                                               tag,
                                               User::Key(other_key),
                                               3,
                                               msg_id,
                                               owner_key));
    expect_success!(routing_rx, msg_id, Response::DelMDataUserPermissions);

    // `other_key` can no longer mutate the entries.
    let key2 = b"key2";
    let value2_v0 = unwrap!(utility::generate_random_vector(10));
    let actions = btree_map![
            key2.to_vec() => EntryAction::Ins(Value {
                content: value2_v0,
                entry_version: 0,
            })
        ];

    let msg_id = MessageId::new();
    unwrap!(routing.mutate_mdata_entries(client_mgr, name, tag, actions, msg_id, other_key));
    expect_failure!(routing_rx,
                    msg_id,
                    Response::MutateMDataEntries,
                    ClientError::AccessDenied);

}

fn setup() -> (Routing, Receiver<Event>, FullId) {
    let full_id = FullId::new();
    let (routing_tx, routing_rx) = mpsc::channel();
    let routing = unwrap!(Routing::new(routing_tx, Some(full_id.clone())));

    // Wait until connection is established.
    match unwrap!(routing_rx.recv_timeout(Duration::from_secs(10))) {
        Event::Connected => (),
        e => panic!("Unexpected event {:?}", e),
    }

    (routing, routing_rx, full_id)
}

// Create account, put it to the network and return `ClientManager` authority for it.
fn create_account(routing: &Routing,
                  routing_rx: &Receiver<Event>,
                  owner_key: sign::PublicKey)
                  -> Authority {
    let account_name = XorName(sha256::hash(&owner_key[..]).0);
    let account_data = unwrap!(MutableData::new(account_name,
                                                TYPE_TAG_SESSION_PACKET,
                                                Default::default(),
                                                Default::default(),
                                                btree_set![owner_key]));

    let msg_id = MessageId::new();
    unwrap!(routing.put_mdata(Authority::ClientManager(account_name),
                              account_data,
                              msg_id,
                              owner_key));
    expect_success!(routing_rx, msg_id, Response::PutMData);

    Authority::ClientManager(account_name)
}

fn do_get_account_info(routing: &Routing,
                       routing_rx: &Receiver<Event>,
                       dst: Authority)
                       -> AccountInfo {
    let msg_id = MessageId::new();
    unwrap!(routing.get_account_info(dst, msg_id));
    expect_success!(routing_rx, msg_id, Response::GetAccountInfo)
}