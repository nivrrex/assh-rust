#!/bin/bash
# 重放：python3 tools/make_fixtures.py fixture-badpw
exec python3 "$(dirname "$0")/../../tools/make_fixtures.py" fixture-badpw
