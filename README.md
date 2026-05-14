# 📡 Wifly — Wi-Fi File Sharing Hotspot

**Wifly** is a modern, ultra-lightweight desktop application **specifically designed for Linux** to share files instantly between your computer and mobile devices without requiring an external internet connection. It leverages `NetworkManager` to turn your PC into a secure, local Wi-Fi Hotspot.

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Tauri](https://img.shields.io/badge/Tauri-v2-green.svg)
![Rust](https://img.shields.io/badge/Backend-Rust-black.svg)
![AlpineJS](https://img.shields.io/badge/Frontend-Alpine.js-blue.svg)
![Linux](https://img.shields.io/badge/Platform-Linux-orange.svg)

---

## ✨ Features

- 🚀 **Local Wi-Fi Hotspot**: Create a Wi-Fi access point directly from the app (uses NetworkManager on Linux).
- 📂 **Two-Way File Sharing**:
  - Send files from your PC to connected mobile devices.
  - Receive files (Upload) from any smartphone via a simple web interface.
- 📱 **Mobile Web Interface**: No app installation needed on the phone. Just scan the QR Code to access the sharing dashboard.
- ⚡ **Local Speed**: Transfers happen over your Wi-Fi chip, reaching speeds much higher than Bluetooth or cloud services.
- 🔒 **Secure**: Your files never leave your local network.
- 🎨 **Premium UI**: Dark, modern, and responsive interface with a full-featured dashboard.

## 🛠️ Tech Stack

- **Frontend**: [Vite](https://vitejs.dev/), [Alpine.js](https://alpinejs.dev/), Tailwind-like Vanilla CSS.
- **Backend**: [Tauri v2](https://tauri.app/) (Rust), [Actix-web](https://actix.rs/) for high-performance file serving.
- **System**: Native integration with NetworkManager (Linux) for Wi-Fi management.

## 🚀 Installation & Development

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Bun](https://bun.sh/) or Node.js
- (Linux only) `network-manager` installed and active.

### Run in Development Mode

1. Clone the repository:
   ```bash
   git clone https://github.com/your-username/wifly.git
   cd wifly
   ```

2. Install dependencies:
   ```bash
   bun install
   ```

3. Launch the app:
   ```bash
   bun run tauri dev
   ```

### Build for Production

To generate an optimized and lightweight binary:
```bash
bun run tauri build
```
*The optimized binary will be located in `src-tauri/target/release/wifly`.*

## 📦 Binary Optimization

The project is pre-configured for maximum performance and minimum size:
- **LTO (Link Time Optimization)**
- **Panic Abort**
- **Symbol Stripping**
- **UPX Compatible** (optional for even further size reduction)

## 📄 License

Distributed under the MIT License. See `LICENSE` for more information.

---

*Developed with ❤️ for borderless file sharing.*
