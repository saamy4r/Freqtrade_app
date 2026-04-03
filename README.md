# Freqtrade Flutter App

A simple, open-source Flutter application for visualizing and monitoring your Freqtrade bots on Android.

## Features

* View open/closed trades with pull-to-refresh on all screens
* Check portfolio statistics and cumulative profit chart
* Monitor bot profit/loss with color-coded trade cards
* Switch between multiple bots
* Interactive price chart with trade entry/exit markers and pan/zoom
* Terminal-style logs screen with color-coded log levels (INFO, WARNING, ERROR, DEBUG)
* Force exit trades via limit order with P/L preview dialog
* Friendly error screen with retry button when the bot is unreachable

## Screenshots
![photo_2025-10-22_19-22-04](https://github.com/user-attachments/assets/6128f770-56b8-463f-89da-3e89846261b7)
![photo_2025-10-22_19-22-09](https://github.com/user-attachments/assets/77d4526b-ae63-43ac-8d03-95050ae226eb)


## Download

| Platform | Download |
|----------|----------|
| Android | [![Download APK](https://img.shields.io/badge/Download-APK-brightgreen?style=for-the-badge&logo=android)](https://github.com/saamy4r/Freqtrade_app/releases/download/v1.1.0/freqtrade-visualizer-v1.1.0.apk) |
| Linux x64 | [![Download Linux](https://img.shields.io/badge/Download-Linux_x64-blue?style=for-the-badge&logo=linux)](https://github.com/saamy4r/Freqtrade_app/releases/download/v1.1.0/freqtrade-visualizer-v1.1.0-linux-x64.tar.gz) |

> All releases: [github.com/saamy4r/Freqtrade_app/releases](https://github.com/saamy4r/Freqtrade_app/releases)

### Android Installation

1. Download the APK using the button above.
2. On your Android device, enable **Install from unknown sources** (Settings → Security).
3. Open the downloaded APK and tap **Install**.

### Linux Installation

1. Download the tarball using the button above.
2. Extract it:
    ```bash
    tar -xzf freqtrade-visualizer-v1.1.0-linux-x64.tar.gz
    ```
3. Run the app:
    ```bash
    ./bundle/freqtrade_app
    ```

## Build from Source

### Prerequisites

* Flutter SDK: [Install Flutter](https://flutter.dev/docs/get-started/install)
* A running Freqtrade instance with the API enabled.

### Steps

1.  **Clone the repository:**
    ```bash
    git clone https://github.com/Saamy4r/Freqtrade_app.git
    cd Freqtrade_app
    ```

2.  **Install dependencies:**
    ```bash
    flutter pub get
    ```

3.  **Run the app:**
    ```bash
    flutter run
    ```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
