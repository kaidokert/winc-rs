// <license>
#ifdef ARDUINO_ARCH_AVR

#include "bsp/include/nm_bsp.h"
#include "bsp/include/nm_bsp_arduino.h"
#include "common/include/nm_common.h"

#define IS_MEGA (defined(ARDUINO_AVR_MEGA) || defined(ARDUINO_AVR_MEGA2560))

static tpfNmBspIsr gpfIsr;

volatile uint8_t *_receivePortRegister;
volatile uint8_t *_pcint_maskreg;
uint8_t _receiveBitMask;
volatile uint8_t prev_pin_read = 1;

uint8_t rx_pin_read()
{
  return *_receivePortRegister & _receiveBitMask;
}

#if !IS_MEGA

#if defined(PCINT0_vect)
ISR(PCINT0_vect)
{
	if (!rx_pin_read() && gpfIsr)
	{
		gpfIsr();
	}
}
#endif

#if defined(PCINT1_vect)
ISR(PCINT1_vect, ISR_ALIASOF(PCINT0_vect));
#endif

#if defined(PCINT2_vect)
ISR(PCINT2_vect, ISR_ALIASOF(PCINT0_vect));
#endif

#if defined(PCINT3_vect)
ISR(PCINT3_vect, ISR_ALIASOF(PCINT0_vect));
#endif

#endif // !IS_MEGA

#if defined(TIMER4_OVF_vect)
ISR(TIMER4_OVF_vect) {
	uint8_t curr_pin_read = rx_pin_read();
	if ((curr_pin_read != prev_pin_read) && !curr_pin_read && gpfIsr)
	{
		gpfIsr();
	}
	prev_pin_read = curr_pin_read;
}

// stategy 3 - start a timer and perform a sort of polling
void attachFakeInterruptToTimer(void) {
	TCCR4B = (1<<CS41);
	TIMSK4 = (1<<TOIE4);
    OCR4C = 0xFF;
}
#else
void attachFakeInterruptToTimer(void) {
}
#endif

// strategy 1 - attach external interrupt to change pin (works on 328)
void attachInterruptToChangePin(int pin) {
	pinMode(pin, INPUT_PULLUP);
	_receiveBitMask = digitalPinToBitMask(pin);
	uint8_t port = digitalPinToPort(pin);
	_receivePortRegister = portInputRegister(port);

	if (!digitalPinToPCICR(pin)) {
		//need to fallback to strategy 2
		attachFakeInterruptToTimer();
		return;
	}

	*digitalPinToPCICR(pin) |= _BV(digitalPinToPCICRbit(pin));
	_pcint_maskreg = digitalPinToPCMSK(pin);
	*_pcint_maskreg |= _BV(digitalPinToPCMSKbit(pin));
}

void detachInterruptToChangePin(int pin) {
    *_pcint_maskreg &= ~(_BV(digitalPinToPCMSKbit(pin)));
}

void attachInterruptMultiArch(uint32_t pin, void *chip_isr, uint32_t mode)
{
	int pin_irq;
	gpfIsr = chip_isr;

	// stategy 0 - attach external interrupt to pin (works on 32u4)
	pin_irq = digitalPinToInterrupt((int)pin);
	if (pin_irq == (int)NOT_AN_INTERRUPT) {
		attachInterruptToChangePin(pin);
		return;
	}

	attachInterrupt(pin_irq, chip_isr, mode);
	return;
}

void detachInterruptMultiArch(uint32_t pin)
{
	int pin_irq;

	pin_irq = digitalPinToInterrupt((int)pin);
	if (pin_irq == (int)NOT_AN_INTERRUPT) {
		detachInterruptToChangePin(pin);
		return;
	}

	detachInterrupt(pin_irq);
}

#endif
