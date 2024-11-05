import socket
import sys
import time

def start_server(port):
    # Retrieve all IP addresses for the hostname
    hostname = socket.gethostname()
    local_ips = socket.gethostbyname_ex(hostname)[2]

    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as server_socket:
        server_socket.bind(('0.0.0.0', port))
        server_socket.listen()
        server_socket.settimeout(1)  # Set a timeout for the accept() call

        print(f"Server listening on 0.0.0.0:{port}")
        print("Available network interface addresses:")
        for ip in local_ips:
            print(f"  - {ip}")

        try:
            while True:
                try:
                    client_socket, client_address = server_socket.accept()
                    print(f"Connected by {client_address}")
                    with client_socket:
                        client_socket.settimeout(1)  # Set a timeout for recv() calls
                        while True:
                            try:
                                data = client_socket.recv(1024)
                                if not data:
                                    break
                                print(f"Received from {client_address}: {data.decode()}")
                                client_socket.sendall(b"HTTP/1.0 200 OK\r\n\r\n")
                                #/client_socket.sendall(data)  # Echo the received data back to the client
                            except socket.timeout:
                                # Timeout every second to allow a check for KeyboardInterrupt
                                continue
                    print(f"Disconnected from {client_address}")
                except socket.timeout:
                    # Timeout every second to allow a check for KeyboardInterrupt
                    continue
        except KeyboardInterrupt:
            print("\nServer shutting down...")

if __name__ == "__main__":
    start_server(12345)  # Replace 12345 with your desired port number
