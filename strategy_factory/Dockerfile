FROM python:3.11-slim

RUN groupadd --system app && useradd --system --gid app app
USER app

WORKDIR /app

COPY requirements.txt .
RUN pip install --no-cache-dir --user -r requirements.txt

COPY factory.py .

ENV PATH="/home/app/.local/bin:${PATH}"

CMD ["python", "factory.py"]
