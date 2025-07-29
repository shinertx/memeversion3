FROM python:3.11-slim

RUN groupadd --system app && useradd --system --gid app app

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

COPY . .

USER app

EXPOSE 5000

CMD ["python", "app.py"]
