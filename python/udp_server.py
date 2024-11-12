import socket
import sys
import time

def start_udp_server(port):
    # Retrieve all IP addresses for the hostname
    hostname = socket.gethostname()
    local_ips = socket.gethostbyname_ex(hostname)[2]

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as server_socket:
        server_socket.bind(('0.0.0.0', port))
        server_socket.settimeout(1)  # Set a timeout for the recvfrom() call

        print(f"UDP server listening on 0.0.0.0:{port}")
        print("Available network interface addresses:")
        for ip in local_ips:
            print(f"  - {ip}")

        try:
            while True:
                try:
                    data, client_address = server_socket.recvfrom(1024)  # Receive data and address from client
                    print(f"Received from {client_address}: {data.decode()}")
                    
                    # Respond to the client
                    response = b"UDP/1.0 200 OK\r\n\r\n"
                    server_socket.sendto(response, client_address)
                except socket.timeout:
                    # Timeout every second to allow a check for KeyboardInterrupt
                    continue
        except KeyboardInterrupt:
            print("\nUDP Server shutting down...")

if __name__ == "__main__":
    start_udp_server(12345)  # Replace 12345 with your desired port number
