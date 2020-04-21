docker-compose ps | grep node | cut -d' ' -f1 | parallel docker exec {} ./witnet node blockchain --epoch=-1 --limit=1
