import socket
import struct

# Multicast settings
MCAST_GRP = '239.255.5.1'  # Multicast group address
MCAST_PORT = 5007          # Multicast port
TTL = 2                    # Time-to-live for packets

# Create UDP socket
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)

# Set socket options
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_TTL, TTL)

# Join multicast group
mreq = struct.pack('4sl', socket.inet_aton(MCAST_GRP), socket.INADDR_ANY)
sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

# Send multicast packet
message = b'Hello, Multicast!'
sock.sendto(message, (MCAST_GRP, MCAST_PORT))

# Clean up
sock.close()
