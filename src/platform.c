/*
 * platform.c - C platform layer called from the Rust application
 *
 * Author:  Sandesh Ghimire <sandesh@soccentric.com>
 * Date:    2026-11-04
 * Target:  NUCLEO-F767ZI (Zephyr board nucleo_f767zi)
 * SRS:     RZCON-004, RZBT-002, RZBT-006, RZFN-003, RZRT-004, RZGEN-004
 *
 * Driver glue only: the Rust modules in src/ own the application logic.
 * The UART and fatal-error code follow the C Zephyr demo
 * (app-c-zephyr) so both builds behave the same.
 */

#include <soc.h>
#include <string.h>
#include <zephyr/arch/cpu.h>
#include <zephyr/arch/arm/arch.h>
#include <zephyr/device.h>
#include <zephyr/drivers/flash.h>
#include <zephyr/drivers/gpio.h>
#include <zephyr/drivers/uart.h>
#include <zephyr/drivers/watchdog.h>
#include <zephyr/fatal.h>
#include <zephyr/init.h>
#include <zephyr/kernel.h>
#include <zephyr/linker/sections.h>
#include <zephyr/storage/flash_map.h>
#include <zephyr/sys/reboot.h>
#include <zephyr/sys/ring_buffer.h>

#include "platform.h"

BUILD_ASSERT(CONFIG_FLASH_LOAD_OFFSET == 0xC000,
	     "RZBT-001: the application must be linked at 0x0800C000");
BUILD_ASSERT(CONFIG_FLASH_LOAD_SIZE == 0xF4000,
	     "RZBT-001: the application slot is 976 KB");

/* ---------------------------------------------------------------- fault */

#define FAULT_MAGIC      0x4641554CUL /* "FAUL" */
#define FAULT_REASON_MAX 8U           /* startup, the last reason */

enum {
	FAULT_NONE = 0,
	FAULT_HARDFAULT,
	FAULT_MEMMANAGE,
	FAULT_BUSFAULT,
	FAULT_USAGEFAULT,
	FAULT_ASSERT,
	FAULT_STACK_OVERFLOW,
	FAULT_MALLOC_FAILED,
	FAULT_STARTUP
};

struct fault_record {
	uint32_t magic;
	uint32_t boot_id;
	struct plat_fault data;
};

static struct fault_record fault_record __noinit;
static uint32_t fault_boot_id __noinit;

#define FAULT_LED_PORT GPIOB
#define FAULT_LED_PIN  14U

static void fault_copy(char *dst, uint32_t dst_size, const char *src)
{
	uint32_t i = 0U;

	if (src != NULL) {
		while ((i + 1U) < dst_size && src[i] != '\0') {
			dst[i] = src[i];
			i++;
		}
	}
	dst[i] = '\0';
}

static bool fault_valid(void)
{
	return fault_record.magic == FAULT_MAGIC &&
	       fault_record.data.reason <= FAULT_REASON_MAX;
}

void plat_fault_boot_start(void)
{
	fault_boot_id++;
	if (fault_boot_id == 0U) {
		fault_boot_id = 1U;
	}
}

bool plat_fault_get(struct plat_fault *out)
{
	if (!fault_valid()) {
		return false;
	}
	*out = fault_record.data;
	out->task[PLAT_FAULT_TASK_LEN - 1U] = '\0';
	out->file[PLAT_FAULT_FILE_LEN - 1U] = '\0';
	return true;
}

bool plat_fault_is_current_boot(void)
{
	return fault_valid() && fault_record.boot_id == fault_boot_id;
}

bool plat_fault_is_previous_boot(void)
{
	uint32_t previous = fault_boot_id - 1U;

	if (previous == 0U) {
		previous = 0xFFFFFFFFUL;
	}
	return fault_valid() && fault_record.boot_id == previous;
}

void plat_fault_clear(void)
{
	fault_record.magic = 0U;
	__DMB();
}

void plat_fatal(void)
{
	k_panic();
	CODE_UNREACHABLE;
}

static void fault_save(uint32_t reason, uint32_t pc, uint32_t lr, uint32_t psr,
		       const char *task)
{
	/* Invalidate first so a reset in the middle of the copy is not
	 * mistaken for a complete record. */
	fault_record.magic = 0U;
	__DMB();

	fault_record.boot_id = fault_boot_id;
	fault_record.data.reason = reason;
	fault_record.data.cfsr = SCB->CFSR;
	fault_record.data.hfsr = SCB->HFSR;
	fault_record.data.mmfar = SCB->MMFAR;
	fault_record.data.bfar = SCB->BFAR;
	fault_record.data.pc = pc;
	fault_record.data.lr = lr;
	fault_record.data.psr = psr;
	fault_record.data.line = 0U;
	fault_copy(fault_record.data.task, PLAT_FAULT_TASK_LEN, task);
	fault_copy(fault_record.data.file, PLAT_FAULT_FILE_LEN, "zephyr");

	__DMB();
	fault_record.magic = FAULT_MAGIC;
	__DSB();
}

static void fault_led3(bool on)
{
	FAULT_LED_PORT->BSRR = on ? (1UL << FAULT_LED_PIN)
				  : ((1UL << FAULT_LED_PIN) << 16U);
}

static void fault_show_and_reset(void)
{
	__disable_irq();

	/* Register-level LED set-up: this must work from any context. */
	RCC->AHB1ENR |= RCC_AHB1ENR_GPIOBEN;
	__DSB();
	uint32_t moder = FAULT_LED_PORT->MODER;

	moder &= ~(3UL << (FAULT_LED_PIN * 2U));
	moder |= (1UL << (FAULT_LED_PIN * 2U));
	FAULT_LED_PORT->MODER = moder;
	__DSB();

	/* Three short flashes, a pause, repeat; roughly 2 s. */
	for (uint32_t cycle = 0U; cycle < 2U; cycle++) {
		for (uint32_t flash = 0U; flash < 3U; flash++) {
			fault_led3(true);
			k_busy_wait(100U * USEC_PER_MSEC);
			fault_led3(false);
			k_busy_wait(100U * USEC_PER_MSEC);
		}
		k_busy_wait(500U * USEC_PER_MSEC);
	}

	sys_reboot(SYS_REBOOT_COLD);
	for (;;) {
		__NOP();
	}
}

static uint32_t fatal_reason(unsigned int reason)
{
	if (reason >= K_ERR_ARM_MEM_GENERIC &&
	    reason <= K_ERR_ARM_MEM_FP_LAZY_STATE_PRESERVATION) {
		return FAULT_MEMMANAGE;
	}
	if (reason >= K_ERR_ARM_BUS_GENERIC &&
	    reason <= K_ERR_ARM_BUS_FP_LAZY_STATE_PRESERVATION) {
		return FAULT_BUSFAULT;
	}
	if (reason >= K_ERR_ARM_USAGE_GENERIC &&
	    reason <= K_ERR_ARM_USAGE_UNDEFINED_INSTRUCTION) {
		return reason == K_ERR_ARM_USAGE_STACK_OVERFLOW
			       ? FAULT_STACK_OVERFLOW
			       : FAULT_USAGEFAULT;
	}
	if (reason >= K_ERR_ARM_SECURE_GENERIC &&
	    reason <= K_ERR_ARM_SECURE_LAZY_STATE_ERROR) {
		return FAULT_MEMMANAGE;
	}

	if (reason == K_ERR_CPU_EXCEPTION) {
		uint32_t cfsr = SCB->CFSR;

		if ((cfsr & SCB_CFSR_MEMFAULTSR_Msk) != 0U) {
			return FAULT_MEMMANAGE;
		}
		if ((cfsr & SCB_CFSR_BUSFAULTSR_Msk) != 0U) {
			return FAULT_BUSFAULT;
		}
		if ((cfsr & SCB_CFSR_USGFAULTSR_Msk) != 0U) {
			return FAULT_USAGEFAULT;
		}
		return FAULT_HARDFAULT;
	}

	switch (reason) {
	case K_ERR_STACK_CHK_FAIL:
		return FAULT_STACK_OVERFLOW;
	case K_ERR_KERNEL_OOPS:
	case K_ERR_KERNEL_PANIC:
		/* A Rust panic ends in k_panic() through rust_panic_wrap(). */
		return FAULT_ASSERT;
	case K_ERR_SPURIOUS_IRQ:
	default:
		return FAULT_STARTUP;
	}
}

void k_sys_fatal_error_handler(unsigned int reason, const struct arch_esf *esf)
{
	const char *task = "kernel";
	uint32_t pc = 0U;
	uint32_t lr = 0U;
	uint32_t psr = 0U;

	if (!k_is_in_isr()) {
		const char *name = k_thread_name_get(k_current_get());

		if (name != NULL && name[0] != '\0') {
			task = name;
		}
	}
	if (esf != NULL) {
		pc = esf->basic.pc;
		lr = esf->basic.lr;
		psr = esf->basic.xpsr;
	}

	fault_save(fatal_reason(reason), pc, lr, psr, task);
	fault_show_and_reset();
}

/* --------------------------------------------------------------- system */

uint32_t plat_vtor(void)
{
	return SCB->VTOR;
}

void plat_reboot(void)
{
	sys_reboot(SYS_REBOOT_COLD);
	CODE_UNREACHABLE;
}

void plat_dcache_invalidate(uintptr_t addr, uint32_t len)
{
	SCB_InvalidateDCache_by_Addr((void *)addr, (int32_t)len);
}

void plat_thread_name(const char *name)
{
	(void)k_thread_name_set(k_current_get(), name);
}

uint32_t plat_stack_unused(void)
{
	size_t unused = 0U;

	if (k_thread_stack_space_get(k_current_get(), &unused) != 0) {
		return 0U;
	}
	return (uint32_t)unused;
}

/* ------------------------------------------------------------- watchdog */

#define WDT_TIMEOUT_MS 4000U

static const struct device *const wdt = DEVICE_DT_GET(DT_ALIAS(watchdog0));
static bool wdt_installed;

/* RZBT-002: BOOT starts the IWDG with a 4 s timeout.  Feed it from the very
 * first Zephyr initialisation step, before drivers and the scheduler. */
static int wdt_early_feed(void)
{
	IWDG->KR = 0x0000AAAAUL;
	return 0;
}

SYS_INIT(wdt_early_feed, PRE_KERNEL_1, 0);

void plat_wdt_feed(void)
{
	if (device_is_ready(wdt)) {
		(void)wdt_feed(wdt, 0);
	}
}

bool plat_wdt_init(void)
{
	if (!device_is_ready(wdt)) {
		return false;
	}

	/* The IWDG cannot be stopped once BOOT has started it; installing a
	 * timeout re-writes PR/RLR through the key sequence. */
	if (!wdt_installed) {
		struct wdt_timeout_cfg cfg = {
			.window = {
				.min = 0U,
				.max = WDT_TIMEOUT_MS,
			},
			.callback = NULL,
			.flags = WDT_FLAG_RESET_SOC,
		};

		if (wdt_install_timeout(wdt, &cfg) < 0) {
			return false;
		}
		wdt_installed = true;
	}

	if (wdt_setup(wdt, 0U) < 0) {
		return false;
	}

	plat_wdt_feed();
	return true;
}

/* ---------------------------------------------------------- LEDs and B1 */

static const struct gpio_dt_spec leds[3] = {
	GPIO_DT_SPEC_GET(DT_ALIAS(led0), gpios),
	GPIO_DT_SPEC_GET(DT_ALIAS(led1), gpios),
	GPIO_DT_SPEC_GET(DT_ALIAS(led2), gpios),
};
static const struct gpio_dt_spec button = GPIO_DT_SPEC_GET(DT_ALIAS(sw0), gpios);
static struct gpio_callback button_cb;
static K_SEM_DEFINE(button_sem, 0, 1);

static void button_isr(const struct device *port, struct gpio_callback *cb,
		       gpio_port_pins_t pins)
{
	ARG_UNUSED(port);
	ARG_UNUSED(cb);
	ARG_UNUSED(pins);

	k_sem_give(&button_sem);
}

bool plat_gpio_init(void)
{
	for (uint32_t i = 0U; i < ARRAY_SIZE(leds); i++) {
		if (!gpio_is_ready_dt(&leds[i]) ||
		    gpio_pin_configure_dt(&leds[i], GPIO_OUTPUT_INACTIVE) != 0) {
			return false;
		}
	}

	/* B1 is high when pressed and has an external pull-down. */
	if (!gpio_is_ready_dt(&button) ||
	    gpio_pin_configure_dt(&button, GPIO_INPUT | GPIO_PULL_DOWN) != 0) {
		return false;
	}
	gpio_init_callback(&button_cb, button_isr, BIT(button.pin));
	if (gpio_add_callback_dt(&button, &button_cb) != 0 ||
	    gpio_pin_interrupt_configure_dt(&button, GPIO_INT_EDGE_BOTH) != 0) {
		return false;
	}
	return true;
}

void plat_led_set(uint32_t led, bool on)
{
	if (led < ARRAY_SIZE(leds)) {
		(void)gpio_pin_set_dt(&leds[led], on ? 1 : 0);
	}
}

bool plat_button_pressed(void)
{
	return gpio_pin_get_raw(button.port, button.pin) > 0;
}

void plat_button_wait(uint32_t timeout_ms)
{
	(void)k_sem_take(&button_sem, K_MSEC(timeout_ms));
}

/* --------------------------------------------------------------- USART3 */

#define UART_RX_RING_SIZE 4096U
#define UART_TX_MAX       2200U
#define UART_DEFAULT_BAUD 115200U
#define UART_FAST_BAUD    921600U

static const struct device *const uart = DEVICE_DT_GET(DT_NODELABEL(usart3));

RING_BUF_DECLARE(uart_rx_ring, UART_RX_RING_SIZE);

static K_SEM_DEFINE(uart_tx_sem, 0, 1);
static K_SEM_DEFINE(uart_rx_sem, 0, 1);

static uint8_t uart_tx_buf[UART_TX_MAX];
static volatile uint32_t uart_tx_len;
static volatile uint32_t uart_tx_pos;
static volatile bool uart_rx_enabled;
static uint32_t uart_baud = UART_DEFAULT_BAUD;

static void uart_clear_errors(void)
{
	uint32_t isr = USART3->ISR;

	if ((isr & (USART_ISR_ORE | USART_ISR_FE | USART_ISR_NE |
		    USART_ISR_PE)) != 0U) {
		volatile uint32_t dr = USART3->RDR;

		(void)dr;
		USART3->ICR = USART_ICR_ORECF | USART_ICR_FECF |
			      USART_ICR_NCF | USART_ICR_PECF;
	}
}

static void uart_isr(const struct device *dev, void *user_data)
{
	ARG_UNUSED(user_data);

	(void)uart_irq_update(dev);

	uart_clear_errors();

	bool received = false;

	while (uart_irq_rx_ready(dev)) {
		uint8_t chunk[32];
		int got = uart_fifo_read(dev, chunk, sizeof(chunk));

		if (got <= 0) {
			break;
		}
		(void)ring_buf_put(&uart_rx_ring, chunk, (uint32_t)got);
		received = true;
	}
	if (received) {
		k_sem_give(&uart_rx_sem);
	}

	if (uart_irq_tx_ready(dev)) {
		int sent = uart_fifo_fill(dev, &uart_tx_buf[uart_tx_pos],
					  (int)(uart_tx_len - uart_tx_pos));

		if (sent > 0) {
			uart_tx_pos += (uint32_t)sent;
		}
		if (uart_tx_pos >= uart_tx_len) {
			uart_irq_tx_disable(dev);
			k_sem_give(&uart_tx_sem);
		}
	}
}

static void uart_rx_start(void)
{
	ring_buf_reset(&uart_rx_ring);
	uart_irq_rx_enable(uart);
	uart_irq_err_enable(uart);
	uart_rx_enabled = true;
}

bool plat_uart_init(void)
{
	if (!device_is_ready(uart)) {
		return false;
	}

	const struct uart_config cfg = {
		.baudrate = UART_DEFAULT_BAUD,
		.parity = UART_CFG_PARITY_NONE,
		.stop_bits = UART_CFG_STOP_BITS_1,
		.data_bits = UART_CFG_DATA_BITS_8,
		.flow_ctrl = UART_CFG_FLOW_CTRL_NONE,
	};

	if (uart_configure(uart, &cfg) != 0) {
		return false;
	}

	uart_irq_callback_set(uart, uart_isr);
	uart_rx_start();
	uart_baud = UART_DEFAULT_BAUD;
	return true;
}

bool plat_uart_set_baud(uint32_t baud)
{
	if (baud != UART_DEFAULT_BAUD && baud != UART_FAST_BAUD) {
		return false;
	}
	if (baud == uart_baud) {
		return true;
	}

	struct uart_config cfg;
	uint32_t old_baud = uart_baud;

	if (uart_config_get(uart, &cfg) != 0) {
		return false;
	}

	uart_irq_err_disable(uart);
	uart_irq_rx_disable(uart);
	uart_rx_enabled = false;

	cfg.baudrate = baud;
	if (uart_configure(uart, &cfg) != 0) {
		cfg.baudrate = old_baud;
		(void)uart_configure(uart, &cfg);
		uart_rx_start();
		return false;
	}

	uart_baud = baud;
	uart_rx_start();
	return true;
}

uint32_t plat_uart_read(uint8_t *dst, uint32_t max_len)
{
	return ring_buf_get(&uart_rx_ring, dst, max_len);
}

void plat_uart_wait(uint32_t timeout_ms)
{
	if (ring_buf_is_empty(&uart_rx_ring)) {
		(void)k_sem_take(&uart_rx_sem, K_MSEC(timeout_ms));
	}
}

bool plat_uart_rx_running(void)
{
	return uart_rx_enabled && device_is_ready(uart);
}

bool plat_uart_send(const uint8_t *data, uint32_t len, uint32_t timeout_ms)
{
	if (len == 0U || data == NULL) {
		return true;
	}
	if (len > sizeof(uart_tx_buf)) {
		return false;
	}

	/* Drop a stale completion, then hand the frame to the ISR. */
	(void)k_sem_take(&uart_tx_sem, K_NO_WAIT);
	memcpy(uart_tx_buf, data, len);
	uart_tx_len = len;
	uart_tx_pos = 0U;

	/* The STM32 driver raises TX interrupts on TC only, and TC is clear
	 * after BOOT and after every frame (TCCF below), so enabling the IRQ
	 * alone never starts a frame.  Load the first byte here, before the
	 * IRQ is enabled; its completion sets TC and the ISR sends the rest. */
	int primed = uart_fifo_fill(uart, uart_tx_buf, (int)len);

	if (primed > 0) {
		uart_tx_pos = (uint32_t)primed;
	}
	uart_irq_tx_enable(uart);

	if (k_sem_take(&uart_tx_sem, K_MSEC(timeout_ms)) != 0) {
		uart_irq_tx_disable(uart);
		return false;
	}

	/* Wait until the last stop bit has left the shift register, so a reset
	 * right after the reply does not cut it off. */
	uint32_t start = k_uptime_get_32();

	while ((USART3->ISR & USART_ISR_TC) == 0U) {
		if ((k_uptime_get_32() - start) > 10U) {
			break;
		}
	}
	USART3->ICR = USART_ICR_TCCF;

	return true;
}

/* ------------------------------------------------------ CONFIRMED record */

#define META_RUNNING_ADDR    0x08008000UL
#define META_SIZE            0x00004000UL
#define META_CONFIRMED_OFFSET 0x140U
#define META_RECORD_SIZE     32U

BUILD_ASSERT(PARTITION_ADDRESS(app_meta_partition) == META_RUNNING_ADDR,
	     "app_meta_partition must alias the running slot metadata sector");
BUILD_ASSERT(PARTITION_SIZE(app_meta_partition) == META_SIZE,
	     "app_meta_partition must cover exactly one metadata sector");

static const struct device *const flash_dev = PARTITION_DEVICE(app_meta_partition);
static const off_t confirmed_offset =
	PARTITION_OFFSET(app_meta_partition) + META_CONFIRMED_OFFSET;

/* The STM32F7 flash driver flushes the D-cache for erase but not for a
 * write; invalidate the one record before every read. */
static void confirmed_invalidate(void)
{
	SCB_InvalidateDCache_by_Addr((void *)(META_RUNNING_ADDR + META_CONFIRMED_OFFSET),
				     META_RECORD_SIZE);
}

bool plat_confirmed_read(uint8_t *out32)
{
	if (!device_is_ready(flash_dev)) {
		return false;
	}
	confirmed_invalidate();
	return flash_read(flash_dev, confirmed_offset, out32, META_RECORD_SIZE) == 0;
}

bool plat_confirmed_write(const uint8_t *record32)
{
	if (!device_is_ready(flash_dev)) {
		return false;
	}

	uint32_t record[META_RECORD_SIZE / 4U];

	memcpy(record, record32, sizeof(record));
	int rc = flash_write(flash_dev, confirmed_offset, record, sizeof(record));

	confirmed_invalidate();
	return rc == 0;
}
