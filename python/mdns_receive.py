import socket
import struct
import binascii

def mdns_receive():
    # mDNS settings
    MCAST_GRP = '224.0.0.251'  # mDNS IPv4 multicast address
    MCAST_PORT = 5353          # mDNS port

    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)

    # Allow multiple sockets to use the same port
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    # Bind to the port (use '' to listen on all interfaces)
    sock.bind(('', MCAST_PORT))

    # Join multicast group
    mreq = struct.pack('4sl', socket.inet_aton(MCAST_GRP), socket.INADDR_ANY)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

    # Receive and print packets as hex
    print(f"Listening for mDNS packets on {MCAST_GRP}:{MCAST_PORT}")
    try:
        while True:
            data, addr = sock.recvfrom(1024)
            # Convert packet data to hex
            hex_data = binascii.hexlify(data).decode('ascii')
            # Format hex output (e.g., 'a1b2c3' -> 'a1 b2 c3' for readability)
            formatted_hex = ' '.join(hex_data[i:i+2] for i in range(0, len(hex_data), 2))
            print(f"Received from {addr}: {formatted_hex}")
    except KeyboardInterrupt:
        print("\nStopping mDNS receiver")
    finally:
        sock.close()

if __name__ == '__main__':
    mdns_receive()
