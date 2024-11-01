# Tabletop 3D scanner
Simple raspberry pi laser triangulation 3D scanner.

## Printing components
All mechanical components can be 3D printed based on CAD files in `models` folder

## Software
The scanner's software is composed of two parts:
- a server, responsible for handling motor and camera
- a client (web-based interface?) to send commands and download the mesh 

## Build prerequisites
```bash
sudo apt install -y libcamera-dev clang
```


## Pinout references
- [Raspberry Pi 5](https://www.hackatronic.com/wp-content/uploads/2024/03/Raspberry-Pi-5-Pinout--1210x642.jpg)
- [TB6612 Motor Driver](https://learn.adafruit.com/adafruit-tb6612-h-bridge-dc-stepper-motor-driver-breakout/pinouts)
- [Complete connection](https://learn.adafruit.com/adafruit-tb6612-h-bridge-dc-stepper-motor-driver-breakout/python-circuitpython)
