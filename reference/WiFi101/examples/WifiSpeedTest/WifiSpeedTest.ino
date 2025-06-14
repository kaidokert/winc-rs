/*
  WiFi Speed Test

  This sketch connects to a web server and downloads a large file
  to measure WiFi throughput using an Arduino WiFi 101 Shield.

  Based on WiFiWebClient example.

  Circuit:
  * WiFi 101 Shield attached to SPI pins

  Created for winc-rs performance benchmarking
*/

#include <SPI.h>
#include <WiFi101.h>
#include "arduino_secrets.h"

// WiFi credentials
char ssid[] = SECRET_SSID;
char pass[] = SECRET_PASS;

int status = WL_IDLE_STATUS;

// Test configuration
char server[] = "kaidokert.com";
// Available test files:
// "/test-file-1mb.json"  - 0.93 MB (2,048 x 512-byte blocks)
// "/test-file-10mb.json" - 9.37 MB (20,480 x 512-byte blocks)
char path[] = "/test-file-1mb.json";
const int port = 80;

WiFiClient client;

// Speed measurement variables
unsigned long startTime = 0;
unsigned long totalBytes = 0;
unsigned long lastReport = 0;
const unsigned long reportInterval = 1000; // Report every 1 second
bool downloadStarted = false;
bool downloadComplete = false;

void setup() {
  Serial.begin(9600);
  while (!Serial) {
    ; // wait for serial port to connect
  }

  // Configure WiFi pins for Feather M0
  WiFi.setPins(8, 7, 4, 2);

  // Check for WiFi shield
  if (WiFi.status() == WL_NO_SHIELD) {
    Serial.println("WiFi 101 Shield not present");
    while (true);
  }

  // Connect to WiFi
  while (status != WL_CONNECTED) {
    Serial.print("Attempting to connect to SSID: ");
    Serial.println(ssid);
    status = WiFi.begin(ssid, pass);
    delay(10000);
  }

  Serial.println("Connected to WiFi");
  printWiFiStatus();

  // Start the speed test
  startSpeedTest();
}

void loop() {
  if (client.connected() || client.available()) {

    // Start timing on first byte received
    if (!downloadStarted && client.available()) {
      downloadStarted = true;
      startTime = millis();
      lastReport = startTime;
      Serial.println("\n=== Download Started ===");
    }

    // Read all available data
    while (client.available()) {
      byte buffer[512];
      int bytesRead = client.readBytes(buffer, sizeof(buffer));
      totalBytes += bytesRead;

      // Periodic progress report
      unsigned long currentTime = millis();
      if (currentTime - lastReport >= reportInterval) {
        reportProgress(currentTime);
        lastReport = currentTime;
      }
    }
  }

  // Check if download is complete
  if (downloadStarted && !client.connected() && !client.available()) {
    if (!downloadComplete) {
      downloadComplete = true;
      finishSpeedTest();
    }
  }
}

void startSpeedTest() {
  Serial.println("\n=== Starting Speed Test ===");
  Serial.print("Connecting to server: ");
  Serial.println(server);

  if (client.connect(server, port)) {
    Serial.println("Connected to server");

    // Send HTTP GET request
    client.print("GET ");
    client.print(path);
    client.println(" HTTP/1.1");
    client.print("Host: ");
    client.println(server);
    client.println("User-Agent: Arduino-WiFi101-SpeedTest/1.0");
    client.println("Connection: close");
    client.println();

    Serial.println("HTTP request sent");
  } else {
    Serial.println("Connection to server failed");
  }
}

void reportProgress(unsigned long currentTime) {
  unsigned long elapsed = currentTime - startTime;
  float seconds = elapsed / 1000.0;
  float kbps = (totalBytes * 8.0) / (seconds * 1000.0); // kilobits per second
  float mbps = kbps / 1000.0; // megabits per second

  Serial.print("Progress: ");
  Serial.print(totalBytes);
  Serial.print(" bytes in ");
  Serial.print(seconds, 1);
  Serial.print(" sec | ");
  Serial.print(kbps, 1);
  Serial.print(" Kbps (");
  Serial.print(mbps, 2);
  Serial.println(" Mbps)");
}

void finishSpeedTest() {
  unsigned long endTime = millis();
  unsigned long totalTime = endTime - startTime;
  float seconds = totalTime / 1000.0;

  Serial.println("\n=== Download Complete ===");
  Serial.print("Total bytes: ");
  Serial.println(totalBytes);
  Serial.print("Total time: ");
  Serial.print(seconds, 2);
  Serial.println(" seconds");

  // Calculate final speeds
  float bytesPerSec = totalBytes / seconds;
  float kbytesPerSec = bytesPerSec / 1024.0;
  float mbytesPerSec = kbytesPerSec / 1024.0;

  float bitsPerSec = totalBytes * 8.0 / seconds;
  float kbps = bitsPerSec / 1000.0;
  float mbps = kbps / 1000.0;

  Serial.println("\n=== Final Results ===");
  Serial.print("Speed: ");
  Serial.print(bytesPerSec, 0);
  Serial.print(" bytes/sec (");
  Serial.print(kbytesPerSec, 1);
  Serial.print(" KB/s, ");
  Serial.print(mbytesPerSec, 2);
  Serial.println(" MB/s)");

  Serial.print("Throughput: ");
  Serial.print(kbps, 1);
  Serial.print(" Kbps (");
  Serial.print(mbps, 2);
  Serial.println(" Mbps)");

  client.stop();

  Serial.println("\nSpeed test complete. Reset to run again.");
  while (true) {
    delay(1000);
  }
}

void printWiFiStatus() {
  Serial.print("SSID: ");
  Serial.println(WiFi.SSID());

  IPAddress ip = WiFi.localIP();
  Serial.print("IP Address: ");
  Serial.println(ip);

  long rssi = WiFi.RSSI();
  Serial.print("Signal strength (RSSI): ");
  Serial.print(rssi);
  Serial.println(" dBm");
}
