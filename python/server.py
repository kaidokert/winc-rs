import socket
import select
import sys
import time

def start_combined_server(base_port, port_range, additional_ports=[]):
    # Retrieve all IP addresses for the hostname
    hostname = socket.gethostname()
    local_ips = socket.gethostbyname_ex(hostname)[2]

    ports = list(range(base_port, base_port + port_range)) + additional_ports

    # Create lists to store TCP and UDP sockets
    tcp_sockets = []
    udp_sockets = []

    # Create a mapping from socket to port number
    socket_to_port = {}

    for port in ports:
        # Create TCP socket
        tcp_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        tcp_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        tcp_sock.bind(('0.0.0.0', port))
        tcp_sock.listen()
        tcp_sock.setblocking(False)  # Set to non-blocking mode

        tcp_sockets.append(tcp_sock)
        socket_to_port[tcp_sock] = port

        # Create UDP socket
        udp_sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        udp_sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        udp_sock.bind(('0.0.0.0', port))
        udp_sock.setblocking(False)  # Set to non-blocking mode

        udp_sockets.append(udp_sock)
        socket_to_port[udp_sock] = port

        print(f"Listening on TCP and UDP port {port}")

    print("Available network interface addresses:")
    for ip in local_ips:
        print(f"  - {ip}")

    # Lists to keep track of client sockets
    client_sockets = []
    client_socket_to_port = {}

    try:
        while True:
            # Prepare the lists of sockets to monitor for reading
            read_sockets = tcp_sockets + udp_sockets + client_sockets

            # Use select to wait for activity on any socket
            readable, _, _ = select.select(read_sockets, [], [], 1)

            for s in readable:
                if s in tcp_sockets:
                    # This is a listening TCP socket, accept new connection
                    client_sock, client_address = s.accept()
                    print(f"TCP Connection from {client_address} on port {s.getsockname()[1]}")
                    client_sock.setblocking(False)
                    client_sockets.append(client_sock)
                    client_socket_to_port[client_sock] = s.getsockname()[1]
                elif s in udp_sockets:
                    # This is a UDP socket
                    data, client_address = s.recvfrom(1024)
                    port = s.getsockname()[1]
                    decoded = data.decode('utf-8', errors='replace')  # or use errors='ignore'
                    print(f"Received UDP from {client_address} on port {port}: {decoded}")

                    # Send response including port number
                    response = f"UDP/1.0 200 OK from port {port}\r\n\r\n".encode()
                    s.sendto(response, client_address)
                elif s in client_sockets:
                    # This is a connected TCP client socket
                    data = s.recv(1024)
                    port = client_socket_to_port[s]
                    if data:
                        print(f"Received TCP from {s.getpeername()} on port {port}: {data.decode()}")

                        # Send response including port number
                        response = f"HTTP/1.0 200 OK from port {port}\r\n\r\n".encode()
                        s.sendall(response)
                    else:
                        # No data, client has closed the connection
                        print(f"TCP Client {s.getpeername()} disconnected")
                        client_sockets.remove(s)
                        del client_socket_to_port[s]
                        s.close()
            # Allow for KeyboardInterrupt
    except KeyboardInterrupt:
        print("\nServer shutting down...")
        for s in tcp_sockets + udp_sockets + client_sockets:
            s.close()

if __name__ == "__main__":
    base_port = 12345
    port_range = 10  # Will listen on ports 12345 to 12354
    additional_ports = [80]  # Include port 80 if desired
    start_combined_server(base_port, port_range, additional_ports)
