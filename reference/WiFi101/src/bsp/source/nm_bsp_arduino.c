// <license>
#include "bsp/include/nm_bsp.h"
#include "bsp/include/nm_bsp_arduino.h"
#include "common/include/nm_common.h"

int8_t gi8Winc1501CsPin = WINC1501_SPI_CS_PIN;
int8_t gi8Winc1501ResetPin = WINC1501_RESET_PIN;
int8_t gi8Winc1501IntnPin = WINC1501_INTN_PIN;
int8_t gi8Winc1501ChipEnPin = WINC1501_CHIP_EN_PIN;

static tpfNmBspIsr gpfIsr;

void __attribute__((weak)) attachInterruptMultiArch(uint32_t pin, void *chip_isr, uint32_t mode)
{
	attachInterrupt(pin, chip_isr, mode);
}

void __attribute__((weak)) detachInterruptMultiArch(uint32_t pin)
{
	detachInterrupt(pin);
}

static void chip_isr(void)
{
	if (gpfIsr) {
		gpfIsr();
	}
}

/*
 *	@fn		init_chip_pins
 *	@brief	Initialize reset, chip enable and wake pin
 *	@author	M.S.M
 *	@date	11 July 2012
 *	@version	1.0
 */
static void init_chip_pins(void)
{
	if (gi8Winc1501ResetPin > -1)
	{
		/* Configure RESETN pin as output. */
		pinMode(gi8Winc1501ResetPin, OUTPUT);
		digitalWrite(gi8Winc1501ResetPin, HIGH);
	}

	/* Configure INTN pins as input. */
	pinMode(gi8Winc1501IntnPin, INPUT);

	if (gi8Winc1501ChipEnPin > -1)
	{
		/* Configure CHIP_EN as pull-up */
		pinMode(gi8Winc1501ChipEnPin, INPUT_PULLUP);
	}
}

static void deinit_chip_pins(void)
{
	if (gi8Winc1501ResetPin > -1)
	{
		digitalWrite(gi8Winc1501ResetPin, LOW);
		pinMode(gi8Winc1501ResetPin, INPUT);
	}

	if (gi8Winc1501ChipEnPin > -1)
	{
		pinMode(gi8Winc1501ChipEnPin, INPUT);
	}
}

/*
 *	@fn		nm_bsp_init
 *	@brief	Initialize BSP
 *	@return	0 in case of success and -1 in case of failure
 *	@author	M.S.M
 *	@date	11 July 2012
 *	@version	1.0
 */
sint8 nm_bsp_init(void)
{
	gpfIsr = NULL;

	init_chip_pins();

	nm_bsp_reset();

	return M2M_SUCCESS;
}

/**
 *	@fn		nm_bsp_deinit
 *	@brief	De-iInitialize BSP
 *	@return	0 in case of success and -1 in case of failure
 *	@author	M. Abdelmawla
 *	@date	11 July 2012
 *	@version	1.0
 */
sint8 nm_bsp_deinit(void)
{
	deinit_chip_pins();

	return M2M_SUCCESS;
}

/**
 *	@fn		nm_bsp_reset
 *	@brief	Reset NMC1500 SoC by setting CHIP_EN and RESET_N signals low,
 *           CHIP_EN high then RESET_N high
 *	@author	M. Abdelmawla
 *	@date	11 July 2012
 *	@version	1.0
 */
void nm_bsp_reset(void)
{
	if (gi8Winc1501ResetPin > -1)
	{
		digitalWrite(gi8Winc1501ResetPin, LOW);
		nm_bsp_sleep(100);
		digitalWrite(gi8Winc1501ResetPin, HIGH);
		nm_bsp_sleep(100);
	}
}

/*
 *	@fn		nm_bsp_sleep
 *	@brief	Sleep in units of mSec
 *	@param[IN]	u32TimeMsec
 *				Time in milliseconds
 *	@author	M.S.M
 *	@date	28 OCT 2013
 *	@version	1.0
 */
void nm_bsp_sleep(uint32 u32TimeMsec)
{
	while (u32TimeMsec--) {
		delay(1);
	}
}

/*
 *	@fn		nm_bsp_register_isr
 *	@brief	Register interrupt service routine
 *	@param[IN]	pfIsr
 *				Pointer to ISR handler
 *	@author	M.S.M
 *	@date	28 OCT 2013
 *	@sa		tpfNmBspIsr
 *	@version	1.0
 */
void nm_bsp_register_isr(tpfNmBspIsr pfIsr)
{
	gpfIsr = pfIsr;
	attachInterruptMultiArch(gi8Winc1501IntnPin, chip_isr, FALLING);
}

/*
 *	@fn		nm_bsp_interrupt_ctrl
 *	@brief	Enable/Disable interrupts
 *	@param[IN]	u8Enable
 *				'0' disable interrupts. '1' enable interrupts
 *	@author	M.S.M
 *	@date	28 OCT 2013
 *	@version	1.0
 */
void nm_bsp_interrupt_ctrl(uint8 u8Enable)
{
	if (u8Enable) {
		attachInterruptMultiArch(gi8Winc1501IntnPin, chip_isr, FALLING);
	} else {
		detachInterruptMultiArch(gi8Winc1501IntnPin);
	}
}
