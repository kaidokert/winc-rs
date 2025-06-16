// <license>
/**@defgroup  BSPDefine Defines
 * @ingroup nm_bsp
 * @{
 */
#ifndef _NM_BSP_INTERNAL_H_
#define _NM_BSP_INTERNAL_H_

#ifdef ARDUINO_ARCH_AVR
#define LIMITED_RAM_DEVICE
#include "bsp/include/nm_bsp_avr.h"
#else
#include "bsp/include/nm_bsp_samd21.h"
#endif

#if defined(ARDUINO) && !defined(ARDUINO_SAMD_MKR1000)
#define CONF_PERIPH
#endif

#endif //_NM_BSP_INTERNAL_H_
