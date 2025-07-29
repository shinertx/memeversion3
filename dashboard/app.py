from flask import Flask, render_template
import redis
import json
import os

app = Flask(__name__)

# Redis connection
redis_client = redis.from_url(os.getenv('REDIS_URL', 'redis://redis:6379'))

@app.route('/')
def dashboard():
    try:
        # Get basic system status
        nav = redis_client.get('metrics:portfolio:nav') or b'0'
        realized_pnl = redis_client.get('metrics:portfolio:realized_pnl') or b'0'
        
        status = {
            'nav': float(nav.decode()),
            'realized_pnl': float(realized_pnl.decode()),
            'redis_connected': True
        }
    except Exception as e:
        status = {
            'nav': 0,
            'realized_pnl': 0,
            'redis_connected': False,
            'error': str(e)
        }
    
    return render_template('index.html', status=status)

@app.route('/health')
def health():
    return {'status': 'ok'}, 200

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=5000, debug=False)
