# Tabletop 3D scanner
Simple raspberry pi laser triangulation 3D scanner.

## Printing components
All mechanical components can be 3D printed based on CAD files in `models` folder

## Software
The scanner's software is composed of two parts:
- a server, responsible for handling motor and camera
- a client (web-based interface?) to send commands and download the mesh 

## Build

### Server
The fastest way to build the server is to build it on a Raspberry Pi 5 with a decent amount of ram (>=4GB) or cross-compile it on a bigger machine (wasn't able to make [cross](https://github.com/cross-rs/cross) work for now, open to suggestions). See `docker/Dockerfile` for build dependencies.

If you want to build on your development machine you can simply run `build.ps1`.  It produces the executable file `target/armv7-unknown-linux-gnueabihf/release/scanner_3d`. By default it builds for Debian Bullseye, change the base docker image in `docker/Dockerfile` if your Raspberry Pi OS is not based on Bullseye.

### UI
To build the UI as a progressive web app you will need `trunk`.
```bash
cargo install --locked trunk
cd scanner_ui
trunk build
```
To test it locally you can use 
```bash
trunk serve
```


## Pinout references
- [Raspberry Pi 5](https://www.hackatronic.com/wp-content/uploads/2024/03/Raspberry-Pi-5-Pinout--1210x642.jpg)
- [TB6612 Motor Driver](https://learn.adafruit.com/adafruit-tb6612-h-bridge-dc-stepper-motor-driver-breakout/pinouts)
- [Complete connection](https://learn.adafruit.com/adafruit-tb6612-h-bridge-dc-stepper-motor-driver-breakout/python-circuitpython)
