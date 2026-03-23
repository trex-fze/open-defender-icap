# Elasticsearch Templates & ILM

Stage 6 introduces an explicit index template + ILM policy for the `traffic-events-*` indices created by the event-ingester.

## Apply template
```bash
curl -u elastic:${ELASTIC_PASSWORD:-changeme-elastic} \
  -H 'Content-Type: application/json' \
  -X PUT http://localhost:9200/_index_template/traffic-events-template \
  -d @deploy/elastic/index-template.json
```

## Apply ILM policy
```bash
curl -u elastic:${ELASTIC_PASSWORD:-changeme-elastic} \
  -H 'Content-Type: application/json' \
  -X PUT http://localhost:9200/_ilm/policy/traffic-events-ilm \
  -d @deploy/elastic/ilm-policy.json
```

After applying, link the template to the ILM policy:
```bash
curl -u elastic:${ELASTIC_PASSWORD:-changeme-elastic} \
  -H 'Content-Type: application/json' \
  -X PUT http://localhost:9200/_data_stream/traffic-events --data '{"template": {"settings": {"index.lifecycle.name": "traffic-events-ilm"}}}'
```
