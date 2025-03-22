use ic_cdk::storage;
use ic_cdk::api::time;
use candid::Principal;
use std::collections::HashMap;
use candid::{CandidType, Deserialize};

type ServerName = String;
type Timestamp = u64;
type IPAddress = String;
type PublicKey = String;
type Reputation = i32;

#[derive(CandidType, Deserialize, Default)]
struct ServerInfo {
    server_name: ServerName,
    public_key: PublicKey,
    ip_address: IPAddress,
    last_active: Timestamp,
    reputation: Reputation,
}

#[derive(CandidType, Deserialize, Default)]
struct ServerRegistry {
    servers: HashMap<Principal, ServerInfo>,
    client_assignments: HashMap<ServerName, PublicKey>,
}

#[ic_cdk::init]
fn init() {
    storage::stable_save((ServerRegistry::default(),)).unwrap();
}

#[ic_cdk::update]
fn register_server(server_name: ServerName, public_key: PublicKey, ip_address: IPAddress) -> String {
    let caller = ic_cdk::caller();
    let now = time() / 1_000_000_000;

    let (mut registry,): (ServerRegistry,) = storage::stable_restore().unwrap_or_else(|_| {
        ic_cdk::println!("Error restoring registry, initializing new one.");
        (ServerRegistry::default(),)
    });

    if registry.servers.contains_key(&caller) {
        return format!("Error: Server already registered by this principal: {}", caller);
    }

    let server_info = ServerInfo {
        server_name: server_name.clone(),
        public_key: public_key.clone(),
        ip_address: ip_address.clone(),
        last_active: now,
        reputation: 0,  // Default reputation
    };

    registry.servers.insert(caller, server_info);
    storage::stable_save((registry,)).unwrap();

    format!("Server '{}' registered successfully with IP: {} by caller: {}", server_name, ip_address, caller)
}

#[ic_cdk::update]
fn update_reputation(principal_id: Principal, delta: Reputation) -> String {
    let (mut registry,): (ServerRegistry,) = storage::stable_restore().unwrap_or_else(|_| {
        ic_cdk::println!("Error restoring registry, initializing new one.");
        (ServerRegistry::default(),)
    });

    if let Some(server_info) = registry.servers.get_mut(&principal_id) {
        server_info.reputation += delta;
        let server_name = server_info.server_name.clone();
        let reputation = server_info.reputation;
        storage::stable_save((registry,)).unwrap();
        format!("Reputation updated for server '{}': New reputation: {}", server_name, reputation)
    } else {
        format!("Error: Server not found for principal ID: {}", principal_id)
    }
}

#[ic_cdk::update]
fn heartbeat() -> String {
    let caller = ic_cdk::caller();
    let now = time() / 1_000_000_000;
    let (mut registry,): (ServerRegistry,) = storage::stable_restore().unwrap_or_else(|_| {
        ic_cdk::println!("Error restoring registry, initializing new one.");
        (ServerRegistry::default(),)
    });

    if let Some(server_info) = registry.servers.get_mut(&caller) {
        if let Some(client_pub_key) = registry.client_assignments.remove(&server_info.server_name) {
            storage::stable_save((registry,)).unwrap();
            return format!("Client {} assigned. Stopping heartbeat. Caller: {}", client_pub_key, caller);
        }

        server_info.last_active = now;
        storage::stable_save((registry,)).unwrap();
        return format!("Heartbeat updated for caller: {}", caller);
    }

    format!("Error: Server not found for caller: {}", caller)
}

#[ic_cdk::update]
fn select_server(server_name: ServerName, client_public_key: PublicKey) -> Result<(PublicKey, IPAddress), String> {
    let (mut registry,): (ServerRegistry,) = storage::stable_restore().unwrap_or_else(|_| {
        ic_cdk::println!("Error restoring registry, initializing new one.");
        (ServerRegistry::default(),)
    });

    let server_entry = registry.servers.values().find(|server_info| server_info.server_name == server_name)
        .map(|server_info| (server_info.public_key.clone(), server_info.ip_address.clone()));

    match server_entry {
        Some((server_public_key, ip_address)) => {
            registry.client_assignments.insert(server_name.clone(), client_public_key.clone());
            storage::stable_save((registry,)).unwrap();
            Ok((server_public_key, ip_address))
        }
        None => Err("Error: Server not found".to_string()),
    }
}

#[ic_cdk::query]
fn get_active_servers() -> Vec<(ServerName, PublicKey, IPAddress, Reputation)> {
    let now = time() / 1_000_000_000;
    let (registry,): (ServerRegistry,) = storage::stable_restore().unwrap_or_else(|_| (ServerRegistry::default(),));

    registry.servers
        .iter()
        .filter(|(_, server_info)| now.saturating_sub(server_info.last_active) <= 30)
        .map(|(_, server_info)| (
            server_info.server_name.clone(),
            server_info.public_key.clone(),
            server_info.ip_address.clone(),
            server_info.reputation,
        ))
        .collect()
}
