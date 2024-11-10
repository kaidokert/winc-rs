import socket
import threading
import struct

class MQTTBroker:
    def __init__(self, host='0.0.0.0', port=1883):
        self.host = host
        self.port = port

    def start(self):
        print(f"Starting MQTT broker on {self.host}:{self.port}")
        server_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server_sock.bind((self.host, self.port))
        server_sock.listen(5)
        try:
            while True:
                client_sock, addr = server_sock.accept()
                print(f"Client connected from {addr}")
                threading.Thread(target=self.handle_client, args=(client_sock,), daemon=True).start()
        finally:
            server_sock.close()

    def handle_client(self, client_sock):
        try:
            while True:
                # Read the fixed header (at least 2 bytes)
                fixed_header = client_sock.recv(2)
                if not fixed_header:
                    break  # Client disconnected

                packet_type = fixed_header[0] >> 4
                remaining_length = fixed_header[1]

                # Read the remaining packet data
                packet_data = client_sock.recv(remaining_length)
                if packet_type == 1:  # CONNECT packet
                    # Send CONNACK packet
                    connack_packet = bytes([0x20, 0x02, 0x00, 0x00])
                    client_sock.sendall(connack_packet)
                    print("Sent CONNACK to client")
                elif packet_type == 3:  # PUBLISH packet
                    # Extract topic length
                    topic_length = struct.unpack('!H', packet_data[:2])[0]
                    topic = packet_data[2:2 + topic_length].decode()
                    message = packet_data[2 + topic_length:].decode()
                    print(f"Received message on topic '{topic}': {message}")
                else:
                    print(f"Received unhandled packet type: {packet_type}")
        except Exception as e:
            print(f"Error handling client: {e}")
        finally:
            client_sock.close()
            print("Client disconnected")

if __name__ == "__main__":
    broker = MQTTBroker()
    broker.start()
