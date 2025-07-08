import socket
import struct
import binascii
import select
import sys

def mdns_receive():
    # mDNS settings
    MCAST_GRP = '224.0.0.251'  # mDNS IPv4 multicast address
    MCAST_PORT = 5353          # mDNS port
    TIMEOUT = 0.5              # Socket timeout in seconds for polling

    # Create UDP socket
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)

    # Set socket timeout for non-blocking receive
    sock.settimeout(TIMEOUT)

    # Allow multiple sockets to use the same port
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    # Bind to the port (use '' to listen on all interfaces)
    sock.bind(('0.0.0.0', MCAST_PORT))

    # Join multicast group
    mreq = struct.pack('4sl', socket.inet_aton(MCAST_GRP), socket.INADDR_ANY)
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

    # Receive and process packets
    print(f"Listening for mDNS packets on {MCAST_GRP}:{MCAST_PORT}")
    print("Press 'q' or Ctrl+C to quit")
    try:
        while True:

            # Poll socket for data
            try:
                data, addr = sock.recvfrom(1024)
                # Convert packet data to hex
                hex_data = binascii.hexlify(data).decode('ascii')
                # Format hex output (e.g., 'a1b2c3' -> 'a1 b2 c3' for readability)
                formatted_hex = ' '.join(hex_data[i:i+2] for i in range(0, len(hex_data), 2))

                # Decode DNS header (first 12 bytes)
                if len(data) >= 12:
                    # Unpack header: 6 unsigned shorts (2 bytes each)
                    trans_id, flags, qdcount, ancount, nscount, arcount = struct.unpack('!HHHHHH', data[:12])

                    # Decode flags
                    qr = (flags >> 15) & 0x1  # Query (0) or Response (1)
                    opcode = (flags >> 11) & 0xF  # Opcode (usually 0 for standard query)
                    aa = (flags >> 10) & 0x1  # Authoritative Answer
                    tc = (flags >> 9) & 0x1  # Truncated
                    rd = (flags >> 8) & 0x1  # Recursion Desired
                    ra = (flags >> 7) & 0x1  # Recursion Available
                    rcode = flags & 0xF  # Response Code

                    # Print decoded header and raw data
                    print(f"\nReceived from {addr[0]}:{addr[1]}")
                    print("DNS Header:")
                    print(f"  Transaction ID: 0x{trans_id:04x}")
                    print(f"  Flags: 0x{flags:04x}")
                    print(f"    QR: {qr} ({'Response' if qr else 'Query'})")
                    print(f"    Opcode: {opcode}")
                    print(f"    AA: {aa} (Authoritative Answer: {'Yes' if aa else 'No'})")
                    print(f"    TC: {tc} (Truncated: {'Yes' if tc else 'No'})")
                    print(f"    RD: {rd} (Recursion Desired: {'Yes' if rd else 'No'})")
                    print(f"    RA: {ra} (Recursion Available: {'Yes' if ra else 'No'})")
                    print(f"    RCODE: {rcode} ({'No Error' if rcode == 0 else 'Error'})")
                    print(f"  Questions: {qdcount}")
                    print(f"  Answers: {ancount}")
                    print(f"  Authority Records: {nscount}")
                    print(f"  Additional Records: {arcount}")
                    print(f"Raw Data: {formatted_hex}")
                else:
                    print(f"\nReceived from {addr[0]}:{addr[1]} (Packet too short for DNS header)")
                    print(f"Raw Data: {formatted_hex}")
            except socket.timeout:
                # No data received in TIMEOUT seconds, continue polling
                continue
            except KeyboardInterrupt:
                print("\nExiting on Ctrl+C")
                break

    finally:
        sock.close()
        print("Socket closed")

if __name__ == '__main__':
    mdns_receive()
