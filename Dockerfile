FROM rust:latest

# Install dependencies
RUN apt-get update && apt-get install -y \
    git \
    && rm -rf /var/lib/apt/lists/*

# Clone ezkl repository
RUN git clone https://github.com/zkonduit/ezkl.git /ezkl

WORKDIR /ezkl

# Build the ezkl tool
RUN cargo install --force --path . \
    && cargo build --release --bin ezkl

# Set the PATH
ENV PATH="/root/.cargo/bin:${PATH}"

# Set up working directory
WORKDIR /app

# Copy scripts to the container
COPY scripts /app/scripts
COPY models /app/models
COPY data /app/data

# Set up entry point (optional, if you want to run something by default)
ENTRYPOINT ["/bin/bash"]