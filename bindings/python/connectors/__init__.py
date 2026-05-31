"""0xDSI-CET connector layer.

The connector layer intentionally keeps ingestion separate from the CET hot path.
Connectors normalize external event streams into the canonical security event
shape consumed by the Spark Declarative Pipeline and CET runtime.
"""
from .base import ConnectorConfig, ConnectorEvent, normalize_connector_event
from .zerobus import ZeroBusClient, ZeroBusConfig, zero_bus_events

__all__ = [
    "ConnectorConfig",
    "ConnectorEvent",
    "normalize_connector_event",
    "ZeroBusClient",
    "ZeroBusConfig",
    "zero_bus_events",
]
