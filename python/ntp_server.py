import socket
import struct
import time
import argparse

def handle_ntp_request(data):
    # Extract the client's transmit timestamp from the request (if available)
    if len(data) >= 48:
        orig_timestamp = struct.unpack('!Q', data[40:48])[0]
    else:
        orig_timestamp = 0
    print("Client's Transmit Timestamp:", orig_timestamp)

    # NTP packet fields
    LI = 0         # Leap Indicator: no warning
    VN = 3         # Version Number: NTP v3
    Mode = 4       # Mode: server
    stratum = 1    # Stratum level
    poll = 0       # Poll interval (not used here)
    precision = 0  # Precision (not used here)

    root_delay = 0
    root_dispersion = 0
    ref_id = b'LOCL'  # Reference ID

    # Current time in NTP timestamp format
    current_time = time.time() + 2208988800  # Convert to NTP epoch
    timestamp = int(current_time * (2**32))

    # Timestamps
    ref_timestamp = timestamp     # Reference Timestamp
    recv_timestamp = timestamp    # Receive Timestamp
    tx_timestamp = timestamp      # Transmit Timestamp

    # Pack the NTP response packet
    packet = struct.pack('!BBBbII4sQQQQ',
                         (LI << 6) | (VN << 3) | Mode,
                         stratum,
                         poll,
                         precision,
                         root_delay,
                         root_dispersion,
                         ref_id,
                         ref_timestamp,
                         orig_timestamp,
                         recv_timestamp,
                         tx_timestamp)
    return packet

def main():
    parser = argparse.ArgumentParser(description='Minimal NTP Server')
    parser.add_argument('--port', type=int, default=123, help='UDP port number to listen on (default: 123)')
    args = parser.parse_args()

    port = args.port

    # Retrieve all IP addresses for the hostname
    hostname = socket.gethostname()
    local_ips = socket.gethostbyname_ex(hostname)[2]
    print("Available network interface addresses:")
    for ip in local_ips:
        print(f"  - {ip}")

    # Create a UDP socket and bind to the specified port
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(('', port))  # May require root privileges if port < 1024

    print(f"NTP server is running on UDP port {port}...")

    while True:
        data, addr = s.recvfrom(1024)
        print("Received NTP request from:", addr)
        response = handle_ntp_request(data)
        s.sendto(response, addr)

if __name__ == '__main__':
    main()
