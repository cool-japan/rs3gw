#!/usr/bin/env python3
"""
Apache Arrow Flight Client Example with Pandas Integration

This example demonstrates how to use rs3gw's Arrow Flight API with PyArrow
and Pandas for high-performance data science workflows.

Requirements:
    pip install pyarrow pandas

Usage:
    python examples/arrow_flight_pandas.py

Features demonstrated:
    - List S3 objects as Pandas DataFrame with proper type hints
    - Download objects using Arrow Flight (zero-copy when possible)
    - Time-series data with datetime indexes
    - Categorical columns for memory efficiency
    - Spark/Dask metadata for distributed computing
"""

import json
import pyarrow as pa
import pyarrow.flight as flight
import pandas as pd
from typing import Optional


class RS3FlightClient:
    """
    High-level client for rs3gw Arrow Flight API with Pandas integration
    """

    def __init__(self, host: str = "localhost", port: int = 9000):
        """
        Initialize the Flight client

        Args:
            host: rs3gw server hostname
            port: rs3gw server port
        """
        location = flight.Location.for_grpc_tcp(host, port)
        self.client = flight.FlightClient(location)
        print(f"Connected to rs3gw Flight server at {host}:{port}")

    def list_objects_as_dataframe(
        self,
        bucket: str,
        prefix: str = "",
        limit: Optional[int] = None,
        use_pandas_types: bool = True
    ) -> pd.DataFrame:
        """
        List S3 objects as a Pandas DataFrame

        Args:
            bucket: S3 bucket name
            prefix: Object key prefix filter
            limit: Maximum number of objects to return
            use_pandas_types: Convert to Pandas-optimized dtypes

        Returns:
            DataFrame with columns: key, size, last_modified, etag, content_type
        """
        # Create flight ticket
        ticket_data = {
            "bucket": bucket,
            "key": prefix if prefix else None,
            "limit": limit
        }

        # Serialize ticket to JSON
        ticket_json = json.dumps(ticket_data)
        ticket = flight.Ticket(ticket_json.encode("utf-8"))

        # Get data stream from server
        reader = self.client.do_get(ticket)

        # Read all batches and convert to Pandas
        table = reader.read_all()
        df = table.to_pandas()

        if use_pandas_types and not df.empty:
            # Apply Pandas-optimized types based on Arrow metadata
            if 'last_modified' in df.columns:
                df['last_modified'] = pd.to_datetime(df['last_modified'])

            if 'content_type' in df.columns:
                df['content_type'] = df['content_type'].astype('category')

            # Set 'key' as index if available
            if 'key' in df.columns:
                df = df.set_index('key')

        return df

    def get_object(self, bucket: str, key: str) -> bytes:
        """
        Download object data using Arrow Flight

        Args:
            bucket: S3 bucket name
            key: Object key

        Returns:
            Object data as bytes
        """
        # Create flight ticket
        ticket_data = {
            "bucket": bucket,
            "key": key
        }

        ticket_json = json.dumps(ticket_data)
        ticket = flight.Ticket(ticket_json.encode("utf-8"))

        # Get data stream
        reader = self.client.do_get(ticket)
        table = reader.read_all()

        # Extract binary data from Arrow table
        data_array = table.column(0)
        return data_array[0].as_py()

    def list_flights(self) -> list:
        """
        List all active Flight streams

        Returns:
            List of active flight IDs
        """
        action = flight.Action("list_flights", b"")
        results = list(self.client.do_action(action))

        if results:
            return json.loads(results[0].body.to_pybytes())
        return []

    def cancel_flight(self, flight_id: str):
        """
        Cancel an active Flight stream

        Args:
            flight_id: Flight identifier to cancel
        """
        action = flight.Action("cancel_flight", flight_id.encode("utf-8"))
        list(self.client.do_action(action))


def example_basic_listing():
    """Example: List objects as DataFrame"""
    print("\n=== Basic Object Listing ===")

    client = RS3FlightClient()

    # List objects in bucket
    df = client.list_objects_as_dataframe("my-bucket", prefix="data/")

    print(f"\nFound {len(df)} objects:")
    print(df.head())

    print("\nDataFrame info:")
    print(df.info())

    # Access objects efficiently
    if not df.empty:
        print(f"\nTotal size: {df['size'].sum()} bytes")
        print(f"Content types: {df['content_type'].value_counts().to_dict()}")


def example_download_objects():
    """Example: Download objects using Flight"""
    print("\n=== Download Objects ===")

    client = RS3FlightClient()

    # Download a specific object
    data = client.get_object("my-bucket", "data/example.txt")
    print(f"Downloaded {len(data)} bytes")
    print(f"Content: {data.decode('utf-8')[:100]}")


def example_timeseries_data():
    """Example: Work with time-series data"""
    print("\n=== Time-Series Data ===")

    client = RS3FlightClient()

    # List log files with datetime parsing
    df = client.list_objects_as_dataframe(
        "logs-bucket",
        prefix="metrics/",
        use_pandas_types=True
    )

    if not df.empty:
        # Time-series operations
        df_sorted = df.sort_values('last_modified')
        print("\nLatest objects:")
        print(df_sorted.tail())

        # Resample by hour (if enough data)
        if len(df) > 1:
            hourly_counts = df.resample('1H', on='last_modified').size()
            print("\nObjects per hour:")
            print(hourly_counts)


def example_spark_dask_integration():
    """Example: Prepare data for Spark/Dask"""
    print("\n=== Spark/Dask Integration ===")

    client = RS3FlightClient()

    # Get data with proper partitioning metadata
    df = client.list_objects_as_dataframe("data-lake", limit=1000)

    if not df.empty:
        # Partition data for distributed processing
        num_partitions = 10
        df['partition'] = pd.cut(df.index.str.hash(), bins=num_partitions, labels=False)

        print(f"\nData partitioned into {num_partitions} partitions")
        print(df.groupby('partition').size())

        # Convert to PyArrow for Spark/Dask
        table = pa.Table.from_pandas(df)
        print(f"\nArrow table schema:")
        print(table.schema)

        # This table can be efficiently passed to:
        # - PySpark: spark.createDataFrame(df)
        # - Dask: dask.dataframe.from_pandas(df, npartitions=num_partitions)


def example_categorical_optimization():
    """Example: Use categorical columns for memory efficiency"""
    print("\n=== Categorical Optimization ===")

    client = RS3FlightClient()

    df = client.list_objects_as_dataframe("my-bucket")

    if not df.empty:
        print(f"\nMemory usage before optimization:")
        print(df.memory_usage(deep=True))

        # content_type is already categorical from Arrow metadata
        print(f"\nContent type categories: {df['content_type'].cat.categories.tolist()}")
        print(f"Category codes: {df['content_type'].cat.codes[:10].tolist()}")

        print(f"\nMemory usage after categorical conversion:")
        print(df.memory_usage(deep=True))


def example_flight_management():
    """Example: Manage active Flight streams"""
    print("\n=== Flight Management ===")

    client = RS3FlightClient()

    # List active flights
    flights = client.list_flights()
    print(f"Active flights: {flights}")

    # Cancel a specific flight (if any exist)
    if flights:
        flight_id = flights[0]
        print(f"Cancelling flight: {flight_id}")
        client.cancel_flight(flight_id)


def main():
    """Run all examples"""
    print("=" * 60)
    print("rs3gw Arrow Flight + Pandas Integration Examples")
    print("=" * 60)

    try:
        # Run examples (uncomment as needed)
        example_basic_listing()
        # example_download_objects()
        # example_timeseries_data()
        # example_spark_dask_integration()
        # example_categorical_optimization()
        # example_flight_management()

    except Exception as e:
        print(f"\nError: {e}")
        print("\nMake sure rs3gw server is running and has test data.")
        print("You may need to adjust bucket names and keys for your setup.")


if __name__ == "__main__":
    main()
