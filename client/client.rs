use std::io::{self, Write}; // Fixed import
use std::process::{Command, Stdio};
use reqwest::blocking::Client;
use serde_json::json;
use std::fs;


const CANISTER_URL: &str = "http://127.0.0.1:4943/?canisterId=br5f7-7uaaa-aaaaa-qaaca-cai&id=bd3sg-teaaa-aaaaa-qaaba-cai";
const VPN_NAME: &str = "oneone";
const NIC_NAME: &str = "one0";
const PORT: u16 = 11111;

fn run_command(command: &str, args: &[&str]) {
    println!("Running: {} {:?}", command, args);
    let status = Command::new(command)
        .args(args)
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        eprintln!("Error: {:?} exited with status {}", command, status);
    }
}

fn install_dependencies() {
    println!("Installing dependencies...");
    run_command("sudo", &["apt-get", "update"]);
    run_command("sudo", &["apt-get", "install", "-y", "wireguard", "iptables"]);
}

fn generate_keys() -> (String, String) {
    println!("Generating oneone keys...");

    // Generate private key
    let private_key_output = Command::new("wg")
        .arg("genkey")
        .output()
        .expect("Failed to generate private key");
    let private_key = String::from_utf8_lossy(&private_key_output.stdout).trim().to_string();

    // Generate public key using the private key
    let mut pubkey_process = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to generate public key");

    {
        let stdin = pubkey_process.stdin.as_mut().expect("Failed to open stdin");
        stdin.write_all(private_key.as_bytes()).expect("Failed to write private key to stdin");
    }

    let public_key_output = pubkey_process
        .wait_with_output()
        .expect("Failed to read public key output");
    let public_key = String::from_utf8_lossy(&public_key_output.stdout).trim().to_string();

    (private_key, public_key)
}


fn configure_wireguard(private_key: &str) {
    println!("Configuring one0.conf...");
    let config = format!(
        "[Interface]\nPrivateKey = {}\nAddress = 10.0.0.2/24\nListenPort = {}\n",
        private_key, PORT
    );
    fs::write("/etc/wireguard/one0.conf", config).expect("Failed to write WireGuard config");
    run_command("sudo", &["wg-quick", "up", NIC_NAME]);
}

fn configure_firewall() {
    println!("Configuring firewall...");
    run_command("sudo", &["iptables", "-A", "INPUT", "-i", NIC_NAME, "-j", "ACCEPT"]);
    run_command("sudo", &["iptables", "-A", "FORWARD", "-i", NIC_NAME, "-j", "ACCEPT"]);
}

fn fetch_servers() -> Vec<(String, String, String)> {
    println!("Fetching active servers...");
    let client = Client::new();
    let response = client
        .get(&format!("{}/get_active_servers", CANISTER_URL))
        .send()
        .expect("Failed to fetch servers");

    // Print the raw response for debugging
    let status = response.status();
    let text = response.text().unwrap_or_else(|_| "Failed to read response body".to_string());
    println!("Response Status: {}", status);
    println!("Raw Response Body: {}", text);

    if status.is_success() {
        let servers: Vec<(String, String, String)> = serde_json::from_str(&text)
            .expect("Failed to parse server response");
        servers
    } else {
        eprintln!("Error fetching servers: {}", status);
        Vec::new()
    }
}


fn display_servers(servers: &Vec<(String, String, String)>) {
    println!("Active Servers:");
    for (index, (name, _public_key, ip)) in servers.iter().enumerate() {
        println!("{}: {} (IP: {})", index + 1, name, ip);
    }
}

fn select_server(client_pub_key: &str) {
    let servers = fetch_servers();
    if servers.is_empty() {
        println!("No active servers found.");
        return;
    }

    display_servers(&servers);
    println!("Select a server by number:");

    let mut choice = String::new();
    io::stdin().read_line(&mut choice).unwrap();
    let choice: usize = choice.trim().parse().unwrap_or(0);

    if choice == 0 || choice > servers.len() {
        println!("Invalid choice.");
        return;
    }

    let (server_name, server_public_key, server_ip) = &servers[choice - 1];
    println!("Selected server: {}", server_name);

    let client = Client::new();
    let payload = json!({
        "server_name": server_name,
        "client_public_key": client_pub_key,
    });

    let response = client
        .post(&format!("{}/select_server", CANISTER_URL))
        .json(&payload)
        .send()
        .expect("Failed to select server");

    if response.status().is_success() {
        println!("Server selection successful! Updating WireGuard config...");
        let config_update = format!(
            "[Peer]\nPublicKey = {}\nEndpoint = {}:{}\nAllowedIPs = 0.0.0.0/0\n",
            server_public_key, server_ip, PORT
        );
        fs::write("/etc/wireguard/one0.conf", config_update).expect("Failed to update config");
        run_command("sudo", &["wg-quick", "down", NIC_NAME]);
        run_command("sudo", &["wg-quick", "up", NIC_NAME]);
    } else {
        println!("Failed to select server: {}", response.status());
    }
}

fn main() {
    println!("Choose an option: (1) Setup (2) Connect");
    let mut choice = String::new();
    io::stdin().read_line(&mut choice).unwrap();

    match choice.trim() {
        "1" => {
            install_dependencies();
            let (private_key, public_key) = generate_keys();
            configure_wireguard(&private_key);
            configure_firewall();
            println!("Setup completed. Your public key: {}", public_key);
        }
        "2" => {
            println!("Enter your public key:");
            let mut public_key = String::new();
            io::stdin().read_line(&mut public_key).unwrap();
            select_server(public_key.trim());
        }
        _ => println!("Invalid choice"),
    }
}