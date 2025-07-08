import socket
import struct

# Multicast settings (must match sender)
MCAST_GRP = '239.255.5.1'  # Same as sender
MCAST_PORT = 5007          # Same as sender

# mdns
# Multicast Address: 224.0.0.251
# Port: 5353
def multicast_recv():
    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)

    # Allow multiple sockets to use the same port
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    # Bind to the port (use '' to listen on all interfaces)
    sock.bind(('', MCAST_PORT))

    # Join multicast group
    mreq = struct.pack('4sl', socket.inet_aton(MCAST_GRP), socket.INADDR_ANY)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

    # Receive and print messages
    print(f"Listening for multicast packets on {MCAST_GRP}:{MCAST_PORT}")
    while True:
        try:
            data, addr = sock.recvfrom(1024)
            print(f"Received: {data.decode()} from {addr}")
        except KeyboardInterrupt:
            sock.close()
            break

# Clean up
sock.close()
