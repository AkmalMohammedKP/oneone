use std::process::{Command, Stdio};
use std::io::{self, Write};
use std::thread::sleep;
use std::time::Duration;
use reqwest::blocking::Client;
use serde_json::json;
use std::fs;

const CANISTER_URL: &str = "http://127.0.0.1:4943/?canisterId=b77ix-eeaaa-aaaaa-qaada-cai&id=bw4dl-smaaa-aaaaa-qaacq-cai";
const VPN_NAME: &str = "oneone";
const NIC_NAME: &str = "one0";
const PORT: u16 = 11111;

fn run_command(command: &str, args: &[&str]) -> bool {
    println!("Running: {} {:?}", command, args);
    let status = Command::new(command)
        .args(args)
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        eprintln!("Error: {} exited with status {:?}", command, status);
        return false;
    }
    true
}

fn install_dependencies() {
    println!("Installing dependencies...");
    run_command("sudo", &["apt-get", "update"]);
    run_command("sudo", &["apt-get", "install", "-y", "wireguard", "iptables"]);
}

fn generate_keys() -> (String, String) {
    println!("Generating oneone keys...");

    let private_key_output = Command::new("wg")
        .arg("genkey")
        .output()
        .expect("Failed to generate private key");
    let private_key = String::from_utf8_lossy(&private_key_output.stdout).trim().to_string();

    let mut public_key_process = Command::new("wg")
        .arg("pubkey")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to generate public key");

    if let Some(ref mut stdin) = public_key_process.stdin {
        stdin.write_all(private_key.as_bytes()).expect("Failed to write private key");
    }

    let public_key_output = public_key_process
        .wait_with_output()
        .expect("Failed to read public key output");

    let public_key = String::from_utf8_lossy(&public_key_output.stdout).trim().to_string();
    (private_key, public_key)
}

fn configure_wireguard(private_key: &str) {
    println!("Configuring WireGuard interface...");
    let config = format!(
        "[Interface]\nPrivateKey = {}\nAddress = 10.0.0.1/24\nListenPort = {}\n",
        private_key, PORT
    );
    fs::write(format!("/etc/wireguard/{}.conf", NIC_NAME), config).expect("Failed to write WireGuard config");
    run_command("sudo", &["wg-quick", "up", NIC_NAME]);
}

fn configure_firewall() {
    println!("Configuring firewall...");
    run_command("sudo", &["iptables", "-A", "INPUT", "-i", NIC_NAME, "-j", "ACCEPT"]);
    run_command("sudo", &["iptables", "-A", "FORWARD", "-i", NIC_NAME, "-j", "ACCEPT"]);
}

fn get_private_ip() -> String {
    println!("Fetching private IP...");
    let output = Command::new("hostname")
        .arg("-I")
        .output()
        .expect("Failed to get IP");
    let ip = String::from_utf8_lossy(&output.stdout).trim().to_string();
    ip.split_whitespace().next().unwrap_or("").to_string()
}

fn get_principal_id() -> String {
    println!("Fetching principal ID...");
    let output = Command::new("dfx")
        .args(&["identity", "get-principal"])
        .output()
        .expect("Failed to execute dfx command");

    if !output.status.success() {
        eprintln!("Error: dfx command failed. Status: {:?}", output.status);
        eprintln!("Stderr: {}", String::from_utf8_lossy(&output.stderr));
        return String::from("Unknown");
    }

    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        eprintln!("Error: Principal ID is empty. Make sure DFX is running and the identity is configured.");
        return String::from("Unknown");
    }

    println!("Principal ID: {}", id);
    id
}

fn register_with_canister(server_name: &str, public_key: &str, private_ip: &str) {
    println!("Registering with canister...");
    let principal_id = get_principal_id();
    println!("Principal ID: {}", principal_id);

    let client = Client::new();
    let payload = json!({
        "server_name": server_name,
        "public_key": public_key,
        "ip_address": private_ip,
        "principal_id": principal_id,
    });

    let response = client
        .post(&format!("{}/register_server", CANISTER_URL))
        .json(&payload)
        .send();

    match response {
        Ok(resp) => {
            let text = resp.text().unwrap_or_else(|_| "No response body".to_string());
            // let text=resp;
            // println!("Registration Response: {:?}", text);

            // Handle the plain text response
            if text.contains("Server registered successfully") {
                println!("Server registered successfully: {:?}", text);
            } else {
                eprintln!("Unexpected registration response: {:?}", text);
            }
        }
        Err(e) => {
            eprintln!("Registration failed: {}", e);
        }
    }
}

fn update_reputation(delta: i32) {
    println!("Updating server reputation...");
    let principal_id = get_principal_id();
    let client = Client::new();
    let payload = json!({
        "principal_id": principal_id,
        "delta": delta,
    });

    let response = client
        .post(format!("{}/update_reputation", CANISTER_URL))
        .json(&payload)
        .send();

    match response {
        Ok(resp) => {
            let text = resp.text().unwrap_or_else(|_| "No response body".to_string());
            println!("Reputation Update Response: {}", text);
        }
        Err(e) => eprintln!("Reputation update failed: {}", e),
    }
}

fn heartbeat() {
    println!("Sending heartbeat...");
    let client = Client::new();
    let payload = json!({ "action": "heartbeat" });

    let response = client
        .post(&format!("{}/heartbeat", CANISTER_URL))
        .json(&payload)
        .send();

    match response {
        Ok(resp) => {
            let text = resp.text().unwrap_or_else(|_| "No response body".to_string());
            println!("Heartbeat Response: {}", text);

            // Handle the plain text response
            if text.contains("Heartbeat updated") {
                println!("Heartbeat successful: {}", text);
            } else if text.contains("Client assigned") {
                println!("Client assigned: {}", text);
            } else {
                eprintln!("Unexpected heartbeat response: {}", text);
            }
        }
        Err(e) => {
            eprintln!("Failed to send heartbeat: {}", e);
        }
    }
}

fn main() {
    println!("Choose an option: (1) Register (2) Login");
    let mut choice = String::new();
    io::stdin().read_line(&mut choice).unwrap();

    match choice.trim() {
        "1" => {
            install_dependencies();
            let (private_key, public_key) = generate_keys();
            configure_wireguard(&private_key);
            configure_firewall();
            let private_ip = get_private_ip();
            println!("Enter server name:");
            let mut server_name = String::new();
            io::stdin().read_line(&mut server_name).unwrap();
            register_with_canister(server_name.trim(), &public_key, &private_ip);
        }
        "2" => {
            println!("Logging in and sending heartbeats...");
            loop {
                heartbeat();
                sleep(Duration::from_secs(30));
            }
        }
        _ => println!("Invalid choice"),
    }
}
