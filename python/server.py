import socket
import select
import sys
import time

def verify_request(data, protocol, peer, port, expected_prefix):
    decoded = data.decode('utf-8', errors='replace')

    if not decoded.startswith(expected_prefix):
        print(f"{protocol} [{peer}:{port}] Invalid request: missing {protocol} header.")
        return False

    if not decoded.endswith("\r\n\r\n"):
        print(f"{protocol} [{peer}:{port}] Invalid request: missing CRLF sequence.")
        return False

    print(f"{protocol} [{peer}:{port}] Valid request received.")
    return True

def on_udp_data(client_address, port, data):
    return verify_request(data, "UDP", client_address, port, "UDP /v1")

def on_tcp_data(peername, port, data):
    return verify_request(data, "TCP", peername, port, "GET / HTTP/1.1")

def start_combined_server(base_port, port_range, additional_ports=None,
                          on_tcp_connect=None, on_tcp_data=None,
                          on_udp_data=None, verbose=True):
    # Avoid mutable default argument - create fresh list if None
    if additional_ports is None:
        additional_ports = []

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
        print(f"Binding TCP socket to 0.0.0.0:{port}")
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

        if verbose:
            print(f"Listening on TCP and UDP port {port}")

    if verbose:
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
                    port = s.getsockname()[1]
                    if verbose:
                        print(f"TCP Connection from {client_address} on port {port}")
                    if on_tcp_connect:
                        on_tcp_connect(client_address, port)
                    client_sock.setblocking(False)
                    client_sockets.append(client_sock)
                    client_socket_to_port[client_sock] = port
                elif s in udp_sockets:
                    # This is a UDP socket
                    data, client_address = s.recvfrom(1024)
                    port = s.getsockname()[1]
                    decoded = data.decode('utf-8', errors='replace')  # or use errors='ignore'
                    if verbose:
                        print(f"Received UDP from {client_address} on port {port}: {decoded}")
                    if on_udp_data and not on_udp_data(client_address, port, data):
                        # Send failure response including port number
                        response = f"UDP/1.0 400 Bad Request from port {port}\r\n\r\n".encode()
                        s.sendto(response, client_address)
                        continue

                    # Send okay response including port number
                    response = f"UDP/1.0 200 OK from port {port}\r\n\r\n".encode()
                    s.sendto(response, client_address)
                elif s in client_sockets:
                    # This is a connected TCP client socket
                    data = s.recv(1024)
                    port = client_socket_to_port[s]
                    if data:
                        if verbose:
                            print(f"Received TCP from {s.getpeername()} on port {port}: {data.decode()}")
                        if on_tcp_data and not on_tcp_data(s.getpeername(), port, data):
                            # Send failure response including port number
                            response = f"HTTP/1.0 400 Bad Request from port {port}\r\n\r\n".encode()
                            s.sendall(response)
                            continue

                        if port==12350:
                            if verbose:
                                print("Intentionally not responding on port 12350")
                        else:
                            # Send response including port number
                            response = f"HTTP/1.0 200 OK from port {port}\r\n\r\n".encode()
                            s.sendall(response)
                    else:
                        # No data, client has closed the connection
                        if verbose:
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
    base_port = 22346
    port_range = 10  # Will listen on ports 22346 to 22355
    additional_ports = [80]  # Include port 80 if desired (in addition to the range above)
    start_combined_server(base_port, port_range, additional_ports,
                          on_tcp_data=on_tcp_data, on_udp_data=on_udp_data)
