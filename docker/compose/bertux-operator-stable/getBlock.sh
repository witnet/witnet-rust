docker-compose ps | grep node | cut -d' ' -f1 | parallel docker exec {} ./witnet node block $1
