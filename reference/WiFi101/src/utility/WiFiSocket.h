// <license>
#ifndef WIFISOCKET_H
#define WIFISOCKET_H

extern "C" {
  #include "socket/include/socket.h"
  #include "socket/include/m2m_socket_host_if.h"
}

#include <Arduino.h>
#include <IPAddress.h>

class WiFiSocketClass {
public:
  WiFiSocketClass();
  virtual ~WiFiSocketClass();

  SOCKET create(uint16 u16Domain, uint8 u8Type, uint8 u8Flags);
  sint8 bind(SOCKET sock, struct sockaddr *pstrAddr, uint8 u8AddrLen);
  sint8 listen(SOCKET sock, uint8 backlog);
  sint8 setopt(SOCKET socket, uint8 u8Level, uint8 option_name, const void *option_value, uint16 u16OptionLen);
  sint8 connect(SOCKET sock, struct sockaddr *pstrAddr, uint8 u8AddrLen);
  uint8 connected(SOCKET sock);
  uint8 listening(SOCKET sock);
  uint8 bound(SOCKET sock);
  int available(SOCKET sock);
  int peek(SOCKET sock);
  int read(SOCKET sock, uint8_t* buf, size_t size);
  size_t write(SOCKET sock, const uint8_t *buf, size_t size);
  sint16 sendto(SOCKET sock, void *pvSendBuffer, uint16 u16SendLength, uint16 flags, struct sockaddr *pstrDestAddr, uint8 u8AddrLen);
  IPAddress remoteIP(SOCKET sock);
  uint16_t remotePort(SOCKET sock);
  sint8 close(SOCKET sock);
  SOCKET accepted(SOCKET sock);
  int hasParent(SOCKET sock, SOCKET child);

  static void eventCallback(SOCKET sock, uint8 u8Msg, void *pvMsg);

private:
  void handleEvent(SOCKET sock, uint8 u8Msg, void *pvMsg);
  int fillRecvBuffer(SOCKET sock);

  struct
  {
    uint8_t state;
    SOCKET parent;
    tstrSocketRecvMsg recvMsg;
    struct {
      uint8_t* data;
      uint8_t* head;
      int length;
    } buffer;
    struct sockaddr _lastSendtoAddr;
  } _info[MAX_SOCKET];
};

extern WiFiSocketClass WiFiSocket;

#endif
