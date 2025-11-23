# Use a lightweight Python base image
FROM python:3.11-slim

# Set environment variables for Python
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1

# Set working directory
WORKDIR /app

# Copy requirements first for caching
COPY requirements.txt .

# Install Python dependencies
RUN pip install --no-cache-dir -r requirements.txt

# Copy the rest of the bot code
COPY . .

# Expose the API port (from config.py / .env)
EXPOSE 6000

# Use JSON array form for CMD (avoids the warning you saw)
CMD ["python", "bot.py"]