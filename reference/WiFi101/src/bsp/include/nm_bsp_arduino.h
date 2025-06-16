// <license>
#ifndef _NM_BSP_ARDUINO_H_
#define _NM_BSP_ARDUINO_H_

#include <stdint.h>

#include <Arduino.h>

/*
 * Arduino variants may redefine those pins.
 * If no pins are specified the following defaults are used:
 *  WINC1501_RESET_PIN   - pin 5
 *  WINC1501_INTN_PIN    - pin 7
 *  WINC1501_CHIP_EN_PIN - not connected (tied to VCC)
 */
#if !defined(WINC1501_RESET_PIN)
  #define WINC1501_RESET_PIN  5
#endif
#if !defined(WINC1501_INTN_PIN)
  #define WINC1501_INTN_PIN   7
#endif
#if !defined(WINC1501_SPI_CS_PIN)
  #define WINC1501_SPI_CS_PIN 10
#endif
#if !defined(WINC1501_CHIP_EN_PIN)
  #define WINC1501_CHIP_EN_PIN -1
#endif

extern int8_t gi8Winc1501CsPin;
extern int8_t gi8Winc1501ResetPin;
extern int8_t gi8Winc1501IntnPin;
extern int8_t gi8Winc1501ChipEnPin;

#endif /* _NM_BSP_ARDUINO_H_ */
