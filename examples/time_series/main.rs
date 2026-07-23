use std::path::PathBuf;
use tuckdb::*;

fn main() {
    let dir = PathBuf::from("/tmp/tuckdb_scenarios");
    std::fs::create_dir_all(&dir).unwrap();

    println!("=== Scenario 2: Time-Series Analytics ===");
    println!("Server metrics: cpu, memory, requests per minute\n");

    let schema = Schema::new(vec![
        Field::new("ts", DataType::Timestamp, false),
        Field::new("server", DataType::Utf8, false),
        Field::new("cpu", DataType::Float64, true),
        Field::new("mem", DataType::Float64, true),
        Field::new("requests", DataType::Int64, true),
    ]);

    let mut table = Table::create("metrics", schema.clone(), dir.clone());

    // Generate time-series data: 3 servers, 10 timestamps each
    println!("Inserting time-series data...");
    let servers = ["web-01", "web-02", "db-01"];
    for (si, server) in servers.iter().enumerate() {
        let mut timestamps: Vec<i64> = Vec::new();
        let mut server_names = Vec::new();
        let mut cpus = Vec::new();
        let mut mems = Vec::new();
        let mut reqs: Vec<i64> = Vec::new();

        for hour in 0..10 {
            let base_ts = 1700000000i64 + hour as i64 * 3600;
            timestamps.push(base_ts);
            server_names.push(server.to_string());
            cpus.push(50.0 + (hour as f64 * 5.0) + (si as f64 * 10.0));
            mems.push(60.0 + (hour as f64 * 2.0) + (si as f64 * 5.0));
            reqs.push(100i64 + hour as i64 * 20 + si as i64 * 50);
        }

        let batch = RecordBatch::new(
            schema.clone(),
            vec![
                Column::new(schema.fields[0].clone(), ColumnData::Timestamp(timestamps)),
                Column::new(schema.fields[1].clone(), ColumnData::Utf8(server_names)),
                Column::new(schema.fields[2].clone(), ColumnData::Float64(cpus)),
                Column::new(schema.fields[3].clone(), ColumnData::Float64(mems)),
                Column::new(schema.fields[4].clone(), ColumnData::Int64(reqs)),
            ],
        );
        table.insert_batch(batch);
    }
    println!(
        "  Inserted {} rows (3 servers × 10 timestamps)\n",
        table.num_rows()
    );

    // Query 1: High CPU servers
    println!("Query 1: Servers with CPU > 70");
    {
        let plan = LogicalPlan::scan("metrics")
            .filter(col("cpu").gt(lit_float(70.0)))
            .project(&["ts", "server", "cpu", "requests"]);
        let mut rs = table.execute(plan);
        let mut count = 0;
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                count += 1;
                let ts = match &batch.columns[0].data {
                    ColumnData::Timestamp(v) => v[row],
                    _ => 0,
                };
                let server = match &batch.columns[1].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let cpu = match &batch.columns[2].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let reqs = match &batch.columns[3].data {
                    ColumnData::Int64(v) => v[row],
                    _ => 0,
                };
                println!("  [{}] {} cpu={:.1}% reqs={}", ts, server, cpu, reqs);
            }
        }
        println!("  Total: {} rows\n", count);
    }

    // Query 2: Average CPU per server
    println!("Query 2: AVG(cpu), AVG(mem), SUM(requests) GROUP BY server");
    {
        let plan = LogicalPlan::scan("metrics")
            .project(&["server", "cpu", "mem", "requests"])
            .aggregate(
                vec![
                    (AggOp::Avg, "cpu", "avg_cpu"),
                    (AggOp::Avg, "mem", "avg_mem"),
                    (AggOp::Sum, "requests", "total_reqs"),
                ],
                vec!["server".to_string()],
            );
        let mut rs = table.execute(plan);
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                let server = match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let avg_cpu = match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let avg_mem = match &batch.columns[2].data {
                    ColumnData::Float64(v) => v[row],
                    _ => 0.0,
                };
                let total_reqs = match &batch.columns[3].data {
                    ColumnData::Float64(v) => v[row] as u64,
                    _ => 0,
                };
                println!(
                    "  {}: avg_cpu={:.1}% avg_mem={:.1}% total_reqs={}",
                    server, avg_cpu, avg_mem, total_reqs
                );
            }
        }
    }
    println!();

    // Query 3: Peak request count
    println!("Query 3: MAX(requests) per server (peak load)");
    {
        let plan = LogicalPlan::scan("metrics")
            .project(&["server", "requests"])
            .aggregate(
                vec![(AggOp::Max, "requests", "peak_reqs")],
                vec!["server".to_string()],
            );
        let mut rs = table.execute(plan);
        while let Some(batch) = rs.next_batch() {
            for row in 0..batch.num_rows {
                let server = match &batch.columns[0].data {
                    ColumnData::Utf8(v) => &v[row],
                    _ => "",
                };
                let peak = match &batch.columns[1].data {
                    ColumnData::Float64(v) => v[row] as u64,
                    _ => 0,
                };
                println!("  {} peak requests: {}", server, peak);
            }
        }
    }
    println!();

    // Persist and verify
    println!("Persisting to disk...");
    table.flush();
    let reloaded = Table::open("metrics", dir);
    println!(
        "Reloaded {} rows, data version: {}\n",
        reloaded.num_rows(),
        reloaded.data_version()
    );

    println!("=== Time-series scenarios completed ===");
}
