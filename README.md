# Zeptos

A tiny runtime for async Rust on microcontrollers.

Zeptos turns the ARM Cortex-M NVIC into an executor for async Rust. It runs entirely in handler mode: `await`ing an interrupt means your task continues execution from that interrupt handler. Execution bounces between interrupt handlers, going to sleep rather than ever returning to thread mode. Because ISRs at the same priority level run to completion without preemption, it's single-threaded, with no need for any synchronization overhead anywhere.

## Hardware support

The core scheduler should work on any ARM Cortex-M part. Zeptos additionally includes a USB stack and driver code for select peripherals on:

 * Raspberry Pi RP2040
 * Atmel / Microchip SAM D11 and SAM D21 series

## Why another async runtime?

This started off because I wanted `async` and USB on SAM D11. [Embassy](https://embassy.dev/) doesn't support the SAM D series, and the synchronous usb-device implementation from [atsamd-hal](https://github.com/atsamd-rs/atsamd/tree/master) doesn't fit in SAM D11's 16KB of flash. I looked at [Lilos](https://github.com/cbiffle/lilos) which is far more minimal, but after going through the generated assembly, I decided it was not minimal enough. On Cortex-M0 Lilos spends quite a bit of code space and time toggling the interrupt flag for critical sections due to the lack of atomics in thumbv6m.

The best way to avoid synchronizing with ISRs is to *be* an ISR. I've found the "state machine driven by interrupts" approach works nicely in C, so letting Rust `async` build the state machine seemed like a natural extension. The result is an interesting and unique point in the embedded Rust design space with the code size I was looking for: Zeptos with USB support fits in about 6KB of flash on SAM D11.

Because it's hyper-optimized for Cortex-M0, it also seemed like a great fit for RP2040 even though code size is much less of a concern there.

I made further unique design choices that may or may not be a good idea depending on your application:

  * Pin multiplexing and peripheral configuration are not enforced by the type system. `unsafe` is only for memory safety.
  * Cargo features enable peripherals. Clocks, resets, and interrupts are configured by Zeptos before calling your `main` function. Objects representing exclusive access to the peripherals are passed to `main` in a `Hardware` struct.
  * Peripheral APIs follow the microcontroller functionality with little abstraction, but also implement the `async-embedded-hal` traits for portability.
  * Tasks can be cancelled synchronously by other tasks. The USB stack makes good use of this feature when enabling and disabling configurations and alternate settings.
  * USB descriptors are defined statically by macros and `const fn` instead of built dynamically at runtime.

## Status

Zeptos should be considered highly experimental. You should probably just use Embassy.

I probably won't merge large additions in scope such as support for other microcontroller families unless they align with my interests, but feel free to open issues and pull requests.

## License

MIT or Apache-2.0, at your option

Zeptos includes code from [Embassy](https://embassy.dev/), [atsamd-hal](https://github.com/atsamd-rs/atsamd/tree/master), [rp-hal](https://github.com/rp-rs/rp-hal), and [rp2040-flash](https://github.com/jannic/rp2040-flash) under MIT or Apache-2.0.
