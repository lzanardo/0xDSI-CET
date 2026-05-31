.PHONY: build-native test-native test-python test-v4 test-v5 test-v6 test-v7 test-all wheel sbom main-ready databricks-validate clean

build-native:
	cmake -S . -B build -DCET_ENABLE_PTHREADS=ON -DCET_ENABLE_MMAP_ARENA=ON
	cmake --build build

test-native: build-native
	ctest --test-dir build --output-on-failure

test-python:
	PYTHONPATH=. python tests/golden_replay_test.py
	PYTHONPATH=. python tests/property_semantics_test.py
	PYTHONPATH=. python tests/temporal_semantics_test.py

test-v4:
	./ci/v4_offline_regression.sh

test-v5:
	./ci/v5_main_ready_regression.sh

test-v6:
	./ci/v6_sdp_zerobus_regression.sh

test-v7:
	./ci/v7_standing_runtime_regression.sh

test-all:
	./ci/all_in_one_v7_regression.sh

wheel:
	./scripts/build_wheel.sh

sbom:
	./scripts/generate_sbom.sh

databricks-validate:
	./scripts/databricks_validate.sh

main-ready:
	./scripts/main_readiness_check.sh
	./ci/v7_standing_runtime_regression.sh

clean:
	rm -rf build dist *.egg-info .pytest_cache
	find . -type d -name __pycache__ -prune -exec rm -rf {} +
