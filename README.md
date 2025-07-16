# Bingo Server

## Overview
Bingo Server is a Rust-based web application designed to host and manage bingo games. It leverages Actix Web for the server framework and integrates with PostgreSQL for database management. The server is designed to be deployed on Shuttle for easy cloud hosting.

## Features
- Host and join bingo rooms with secure authentication
- Real-time WebSocket communication for game interactions
- User authentication with Argon2 password hashing
- Session management with secure cookies
- Cross-Origin Resource Sharing (CORS) support for web clients
- Database migrations for easy schema management
- Room token-based access control

## Requirements
- Rust (latest stable version)
- PostgreSQL database
- Cargo for dependency management
- Shuttle CLI for deployment

## Installation
1. Clone the repository:
   ```sh
   git clone https://github.com/web2098/bingoserver.git
   ```
2. Navigate to the project directory:
   ```sh
   cd bingoserver
   ```
3. Install dependencies:
   ```sh
   cargo fetch
   ```

## Database Setup
The project uses PostgreSQL with the following tables:
- `rooms`: Stores room information with host and token
- `users`: Stores user credentials with UUID primary keys

Database migrations are located in the `migrations/` folder and are automatically applied on startup.

## Usage
### Running Locally
1. Set up a PostgreSQL database
2. Run the server:
   ```sh
   cargo run
   ```

### Testing
Run the tests:
```sh
cargo test
```

## API Endpoints
- `GET /host`: Authenticate and create a hosting session
- `GET /start/{room}`: Start a WebSocket connection as a room host
- `GET /join/{room}`: Join a room as a participant

## Deployment
This project is configured for deployment using Shuttle:

1. Install Shuttle CLI:
   ```sh
   cargo install shuttle-cli
   ```
2. Deploy the project:
   ```sh
   shuttle deploy
   ```

## Contributing
Contributions are welcome! Please submit a pull request or open an issue for any bugs or feature requests.

## License
This project is licensed under the MIT License. See the `LICENSE` file for details.