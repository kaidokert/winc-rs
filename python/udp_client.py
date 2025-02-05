import argparse
import socket
import time

if __name__ == "__main__":
    parser = argparse.ArgumentParser()

    parser.add_argument("--port", type=int, default=12345)
    parser.add_argument("--host", type=str, default="localhost")
    args = parser.parse_args()

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as client_socket:

        client_socket.sendto(b"Hello, server!", (args.host, args.port))
        # set socket as nonblocking
        client_socket.settimeout(0)
        # Wait for 1 second, polling
        start_time = time.time()
        while time.time() - start_time < 1:
            try:
                data, _ = client_socket.recvfrom(1024)
                print(f"Received from server: {data.decode()}")
                break
            except socket.error:
                pass
