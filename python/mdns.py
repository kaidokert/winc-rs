import socket
import struct
import time
import argparse
import logging
import sys

log = logging.getLogger(__name__)

SOCKET_RCV_TIMEO = 5 # 5 seconds
SLEEP_TIMEO = 0.600 # 600 milliseconds
RCV_BUFFER_SIZE = 256 # Max message size to receive

def build_mdns_query(service_name: str):
    header = b'\x00\x00'      # Transaction ID
    header += b'\x00\x00'     # Flags
    header += b'\x00\x01'     # Questions
    header += b'\x00\x00'     # Answer RRs
    header += b'\x00\x00'     # Authority RRs
    header += b'\x00\x00'     # Additional RRs

    qname = b''
    for part in service_name.split('.'):
        qname += struct.pack('B', len(part)) + part.encode('utf-8')
    qname += b'\x00'

    qtype = b'\x00\x0c'  # PTR
    qclass = b'\x00\x01'  # IN

    return header + qname + qtype + qclass

def parse_dns_name(data, offset):
    labels = []
    while True:
        length = data[offset]
        if length == 0:
            offset += 1
            break
        if (length & 0xC0) == 0xC0: # Check if name is compressed
            pointer = struct.unpack('>H', data[offset:offset+2])[0]
            pointer &= 0x3FFF  # Mask out the top two bits to get the actual offset
            if pointer >= len(data):
                raise ValueError("Invalid DNS pointer offset")
            sublabels, _ = parse_dns_name(data, pointer)
            labels.extend(sublabels)
            offset += 2
            break
        else:
            offset += 1
            labels.append(data[offset:offset+length].decode())
            offset += length
    return labels, offset

def parse_response(data, service_name: str) -> bool:
    try:
        # Check for question counts
        qdcount = struct.unpack(">H", data[4:6])[0]
        # Check for answers count
        ancount = struct.unpack(">H", data[6:8])[0]
        # check for addtional record count
        arcount = struct.unpack(">H", data[10:12])[0]
        offset = 12

        # feather board does not send any questions in response
        if qdcount != 0:
            return False
        # Skip Questions
        #for _ in range(qdcount):
        #    _, offset = parse_dns_name(data, offset)
        #    offset += 4

        answers = arcount + ancount
        log.debug("Response received: {} answer(s)".format(answers))

        # Parse Answers
        for _ in range(answers):
            name, offset = parse_dns_name(data, offset)
            rtype = struct.unpack(">H", data[offset: offset+2])[0]
            rclass = struct.unpack(">H", data[offset+2: offset+4])[0]
            ttl = struct.unpack(">I", data[offset+4: offset+8])[0]
            rdlength = struct.unpack(">H", data[offset+8: offset+10])[0]
            rdata = data[offset+10: offset+10+rdlength]
            offset += 10 + rdlength

            rcvd_inst_name = ".".join(name)

            log.info("Answer: Name = {}, Type = {}, Class = {}, TTL = {}".format(rcvd_inst_name, rtype, rclass, ttl))

            if rtype == 12 and rcvd_inst_name == service_name:  # PTR
                ptr_name, _ = parse_dns_name(rdata, 0)
                log.info("  PTR -> {}".format(".".join(ptr_name)))
            elif rtype == 33:  # SRV
                priority, weight, port = struct.unpack(">HHH", rdata[:6])
                target, _ = parse_dns_name(rdata, 6)
                log.info("  SRV -> {}:{} (priority {}, weight {})".format(".".join(target), port, priority, weight))
            elif rtype == 1:  # A
                ip = ".".join(map(str, rdata))
                log.info("  A -> {}".format(ip))
            else:
                log.error("Unknown record type or unparsed data.")
                return False

        return True
    except Exception as e:
        log.error("Error parsing DNS response: {}".format(e))
        return False

def send_mdns_query_loop():
    parser = argparse.ArgumentParser(description='Send MDNS requests to a multicast group')
    parser.add_argument('--host', type=str, default='224.0.0.251', help='Multicast group address')
    parser.add_argument('--port', type=int, default=5353, help='Port number')
    parser.add_argument('--log_level', choices=["INFO", "DEBUG"], default="INFO", help='Log level')
    parser.add_argument('--label', type=str, default='_brrdino._tcp.local', help='Service name')

    # parse the arguments
    args = parser.parse_args()

    host = args.host
    port = args.port
    service_name = args.label
    logging.basicConfig(level=args.log_level, stream=sys.stdout)

    # Create the UDP socket and set the its configurations.
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_UDP)
    # re-use the previous ip address.
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    # do not hear yourself
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_MULTICAST_LOOP, 0)
    # listen on all interfaces
    sock.bind(('', port))

    group = socket.inet_aton(host)
    mreq = struct.pack('4sL', group, socket.INADDR_ANY)
    # join the multicast
    sock.setsockopt(socket.IPPROTO_IP, socket.IP_ADD_MEMBERSHIP, mreq)

    log.info("[{}] Starting mDNS query with service name: {}\n".format(time.strftime('%H:%M:%S'), service_name))

    try:
        while True:
            query = build_mdns_query(service_name)
            sock.sendto(query, (host, port))
            log.debug("[{}] Sent mDNS query".format(time.strftime('%H:%M:%S')))

            sock.settimeout(SOCKET_RCV_TIMEO)
            try:
                data, addr = sock.recvfrom(RCV_BUFFER_SIZE)
                if parse_response(data, service_name):
                    log.info("Received response from {}\n".format(addr))
                else:
                    log.debug("[{}] Packet is not from feather board".format(time.strftime('%H:%M:%S')))

            except socket.timeout:
                log.debug("[{}] No response received.".format(time.strftime('%H:%M:%S')))

            time.sleep(SLEEP_TIMEO)

    except KeyboardInterrupt:
        log.info("\nStopped by user.")
    finally:
        sock.close()

if __name__ == "__main__":
    send_mdns_query_loop()
