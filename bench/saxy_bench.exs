# Saxy Benchmark: RustyXML SAX API vs Saxy
#
# Usage:
#   mix run bench/saxy_bench.exs
#
# Compares RustyXML's Saxy-compatible SAX API against Saxy directly.

defmodule SaxyBench do
  @output_dir "bench/results"

  # =========================================================================
  # Shared Handlers — identical behaviour/callback for both libraries
  # =========================================================================

  defmodule RustyCountHandler do
    @behaviour RustyXML.Handler
    def handle_event(:start_element, _, count), do: {:ok, count + 1}
    def handle_event(_, _, count), do: {:ok, count}
  end

  defmodule SaxyCountHandler do
    @behaviour Saxy.Handler
    def handle_event(:start_element, _, count), do: {:ok, count + 1}
    def handle_event(_, _, count), do: {:ok, count}
  end

  defmodule RustyCollectHandler do
    @behaviour RustyXML.Handler
    def handle_event(:start_element, {name, _attrs}, acc), do: {:ok, [name | acc]}
    def handle_event(:characters, text, acc), do: {:ok, [{:text, text} | acc]}
    def handle_event(_, _, acc), do: {:ok, acc}
  end

  defmodule SaxyCollectHandler do
    @behaviour Saxy.Handler
    def handle_event(:start_element, {name, _attrs}, acc), do: {:ok, [name | acc]}
    def handle_event(:characters, text, acc), do: {:ok, [{:text, text} | acc]}
    def handle_event(_, _, acc), do: {:ok, acc}
  end

  def run do
    File.mkdir_p!(@output_dir)
    timestamp = DateTime.utc_now() |> DateTime.to_iso8601(:basic) |> String.slice(0..14)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("SAXY BENCHMARK")
    IO.puts("RustyXML SAX API vs Saxy")
    IO.puts("Timestamp: #{timestamp}")
    IO.puts(String.duplicate("=", 70))

    print_system_info()
    memory_tracking_enabled = check_memory_tracking()
    test_docs = generate_test_docs()

    all_results = []

    # 1. SAX parse_string benchmarks
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("SAX PARSE_STRING BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    parse_small = run_benchmark("parse_string", "Small (50 items)", test_docs.small, fn xml ->
      {fn -> RustyXML.parse_string(xml, RustyCountHandler, 0) end,
       fn -> Saxy.parse_string(xml, SaxyCountHandler, 0) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_small]

    parse_medium = run_benchmark("parse_string", "Medium (1K items)", test_docs.medium, fn xml ->
      {fn -> RustyXML.parse_string(xml, RustyCountHandler, 0) end,
       fn -> Saxy.parse_string(xml, SaxyCountHandler, 0) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_medium]

    parse_large = run_benchmark("parse_string", "Large (10K items)", test_docs.large, fn xml ->
      {fn -> RustyXML.parse_string(xml, RustyCountHandler, 0) end,
       fn -> Saxy.parse_string(xml, SaxyCountHandler, 0) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_large]

    # 2. SAX parse_string with collect handler (more realistic workload)
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("SAX COLLECT BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    collect_medium = run_benchmark("parse_string (collect)", "Medium (1K items)", test_docs.medium, fn xml ->
      {fn -> RustyXML.parse_string(xml, RustyCollectHandler, []) end,
       fn -> Saxy.parse_string(xml, SaxyCollectHandler, []) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [collect_medium]

    # 3. SimpleForm benchmarks
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("SIMPLEFORM BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    simple_small = run_benchmark("SimpleForm", "Small (50 items)", test_docs.small, fn xml ->
      {fn -> RustyXML.SimpleForm.parse_string(xml) end,
       fn -> Saxy.SimpleForm.parse_string(xml) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [simple_small]

    simple_medium = run_benchmark("SimpleForm", "Medium (1K items)", test_docs.medium, fn xml ->
      {fn -> RustyXML.SimpleForm.parse_string(xml) end,
       fn -> Saxy.SimpleForm.parse_string(xml) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [simple_medium]

    # 4. Streaming (parse_stream) benchmark
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("STREAMING BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    streaming_result = run_streaming_benchmark(
      test_docs.large_path, test_docs.large_item_count, memory_tracking_enabled
    )

    # 5. Correctness verification
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("CORRECTNESS VERIFICATION")
    IO.puts(String.duplicate("=", 70))

    correctness = verify_correctness(test_docs.medium)

    # Save results
    save_results(timestamp, all_results, streaming_result, correctness, memory_tracking_enabled)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("BENCHMARK COMPLETE")
    IO.puts("Results saved to: #{@output_dir}/#{timestamp}_saxy_summary.md")
    IO.puts(String.duplicate("=", 70))
  end

  # =========================================================================
  # System Info
  # =========================================================================

  defp print_system_info do
    IO.puts("\n--- System Information ---")
    IO.puts("Elixir: #{System.version()}")
    IO.puts("OTP: #{System.otp_release()}")
    IO.puts("Schedulers: #{System.schedulers_online()}")
    IO.puts("RustyXML: #{Application.spec(:rusty_xml, :vsn)}")
    IO.puts("Saxy: #{Application.spec(:saxy, :vsn)}")
  end

  defp check_memory_tracking do
    IO.puts("\n--- Memory Tracking ---")
    RustyXML.Native.reset_rust_memory_stats()
    _ = RustyXML.parse("<root><item/></root>")
    peak = RustyXML.Native.get_rust_memory_peak()

    if peak > 0 do
      IO.puts("NIF memory tracking: ENABLED")
      true
    else
      IO.puts("NIF memory tracking: DISABLED")
      false
    end
  end

  # =========================================================================
  # Test Data
  # =========================================================================

  defp generate_test_docs do
    IO.puts("\n--- Generating Test Documents ---")
    File.mkdir_p!("bench/data")

    small = ensure_xml_file("bench/data/small.xml", 50)
    IO.puts("Small XML: #{format_size(byte_size(small))} (50 items)")

    medium = ensure_xml_file("bench/data/medium.xml", 1_000)
    IO.puts("Medium XML: #{format_size(byte_size(medium))} (1,000 items)")

    large_path = "bench/data/large.xml"
    large = ensure_xml_file(large_path, 10_000)
    IO.puts("Large XML: #{format_size(byte_size(large))} (10,000 items)")

    %{
      small: small,
      medium: medium,
      large: large,
      large_path: large_path,
      large_item_count: 10_000
    }
  end

  defp generate_xml(count) do
    items =
      1..count
      |> Enum.map(fn i ->
        """
        <item id="#{i}" category="cat#{rem(i, 10)}">
          <name>Product #{i}</name>
          <description>This is a detailed description for product #{i}.</description>
          <price currency="USD">#{:rand.uniform(10000) / 100}</price>
          <quantity>#{:rand.uniform(100)}</quantity>
        </item>
        """
      end)
      |> Enum.join("\n")

    """
    <?xml version="1.0" encoding="UTF-8"?>
    <catalog xmlns="http://example.com/catalog" version="1.0">
      <items>
    #{items}
      </items>
    </catalog>
    """
  end

  defp ensure_xml_file(path, count) do
    if File.exists?(path) do
      File.read!(path)
    else
      xml = generate_xml(count)
      File.write!(path, xml)
      xml
    end
  end

  # =========================================================================
  # Benchmark Runner
  # =========================================================================

  defp run_benchmark(operation, scenario, xml, bench_fn, memory_tracking_enabled) do
    IO.puts("\n--- #{operation}: #{scenario} ---")
    IO.puts("XML Size: #{format_size(byte_size(xml))}")

    {rusty_fn, saxy_fn} = bench_fn.(xml)

    # Warm up
    _ = rusty_fn.()
    _ = saxy_fn.()

    # NIF peak memory
    nif_peak = if memory_tracking_enabled do
      :erlang.garbage_collect()
      Process.sleep(10)
      {baseline, _} = RustyXML.Native.reset_rust_memory_stats()
      _ = rusty_fn.()
      peak = RustyXML.Native.get_rust_memory_peak()
      max(0, peak - baseline)
    else
      0
    end

    result = Benchee.run(
      %{
        "RustyXML" => rusty_fn,
        "Saxy" => saxy_fn
      },
      warmup: 1,
      time: 3,
      memory_time: 1,
      print: [configuration: false]
    )

    rusty = Enum.find(result.scenarios, &(&1.name == "RustyXML"))
    saxy = Enum.find(result.scenarios, &(&1.name == "Saxy"))

    rusty_ips = rusty.run_time_data.statistics.ips
    saxy_ips = saxy.run_time_data.statistics.ips
    rusty_avg = rusty.run_time_data.statistics.average / 1000
    saxy_avg = saxy.run_time_data.statistics.average / 1000

    rusty_beam_mem = get_memory_stat(rusty)
    saxy_beam_mem = get_memory_stat(saxy)

    speedup = if saxy_ips > 0, do: Float.round(rusty_ips / saxy_ips, 2), else: 0

    IO.puts("\nResults:")
    IO.puts("  RustyXML: #{format_ips(rusty_ips)} ips (#{format_time_us(rusty_avg)})")
    IO.puts("  Saxy:     #{format_ips(saxy_ips)} ips (#{format_time_us(saxy_avg)})")
    IO.puts("  Speedup:  #{speedup}x")

    if memory_tracking_enabled do
      IO.puts("\nMemory:")
      IO.puts("  NIF Peak: #{format_size(nif_peak)}")
      IO.puts("  BEAM (RustyXML): #{format_size(rusty_beam_mem)}")
      IO.puts("  BEAM (Saxy): #{format_size(saxy_beam_mem)}")
    end

    %{
      operation: operation,
      scenario: scenario,
      xml_size: byte_size(xml),
      rusty_ips: rusty_ips,
      saxy_ips: saxy_ips,
      rusty_avg_us: rusty_avg,
      saxy_avg_us: saxy_avg,
      speedup: speedup,
      nif_peak: nif_peak,
      rusty_beam_mem: rusty_beam_mem,
      saxy_beam_mem: saxy_beam_mem
    }
  end

  defp get_memory_stat(scenario) do
    case scenario.memory_usage_data do
      nil -> 0
      data ->
        case data.statistics do
          nil -> 0
          stats -> trunc(stats.average || 0)
        end
    end
  end

  # =========================================================================
  # Streaming Benchmark
  # =========================================================================

  defp run_streaming_benchmark(path, expected_count, memory_tracking_enabled) do
    file_size = File.stat!(path).size
    IO.puts("\n--- parse_stream: Large (10K items) ---")
    IO.puts("File: #{path}")
    IO.puts("Size: #{format_size(file_size)}")

    rusty_fn = fn ->
      File.stream!(path, [], 64 * 1024)
      |> RustyXML.parse_stream(SaxyBench.RustyCountHandler, 0)
    end

    saxy_fn = fn ->
      File.stream!(path, [], 64 * 1024)
      |> Saxy.parse_stream(SaxyBench.SaxyCountHandler, 0)
    end

    # Warm up + verify counts
    {:ok, rusty_count} = rusty_fn.()
    {:ok, saxy_count} = saxy_fn.()

    IO.puts("  RustyXML elements: #{format_number(rusty_count)}")
    IO.puts("  Saxy elements: #{format_number(saxy_count)}")

    # NIF peak memory (isolated: GC + reset baseline + single run)
    nif_peak = if memory_tracking_enabled do
      :erlang.garbage_collect()
      Process.sleep(10)
      {baseline, _} = RustyXML.Native.reset_rust_memory_stats()
      rusty_fn.()
      peak = RustyXML.Native.get_rust_memory_peak()
      max(0, peak - baseline)
    else
      0
    end

    # BEAM memory: :erlang.memory(:total) delta (same method as sweet_bench)
    :erlang.garbage_collect()
    rusty_mem_before = :erlang.memory(:total)
    {rusty_time, _} = :timer.tc(fn -> rusty_fn.() end)
    rusty_mem_after = :erlang.memory(:total)
    rusty_beam_mem = max(0, rusty_mem_after - rusty_mem_before)

    :erlang.garbage_collect()
    saxy_mem_before = :erlang.memory(:total)
    {saxy_time, _} = :timer.tc(fn -> saxy_fn.() end)
    saxy_mem_after = :erlang.memory(:total)
    saxy_beam_mem = max(0, saxy_mem_after - saxy_mem_before)

    speedup = if saxy_time > 0, do: Float.round(saxy_time / rusty_time, 2), else: 0

    IO.puts("\nRustyXML parse_stream:")
    IO.puts("  Elements: #{format_number(rusty_count)}")
    IO.puts("  Time: #{format_time(rusty_time)}")

    IO.puts("\nSaxy parse_stream:")
    IO.puts("  Elements: #{format_number(saxy_count)}")
    IO.puts("  Time: #{format_time(saxy_time)}")

    IO.puts("\nComparison:")
    IO.puts("  Speedup: #{speedup}x faster")

    if memory_tracking_enabled do
      IO.puts("\nMemory:")
      IO.puts("  NIF Peak: #{format_size(nif_peak)}")
      IO.puts("  BEAM (RustyXML): #{format_size(rusty_beam_mem)}")
      IO.puts("  BEAM (Saxy): #{format_size(saxy_beam_mem)}")
    end

    %{
      operation: "parse_stream",
      scenario: "Large (10K items)",
      xml_size: file_size,
      rusty_ips: 0,
      saxy_ips: 0,
      rusty_time_us: rusty_time,
      saxy_time_us: saxy_time,
      rusty_count: rusty_count,
      saxy_count: saxy_count,
      speedup: speedup,
      nif_peak: nif_peak,
      rusty_beam_mem: rusty_beam_mem,
      saxy_beam_mem: saxy_beam_mem,
      expected_count: expected_count
    }
  end

  # =========================================================================
  # Correctness
  # =========================================================================

  defp verify_correctness(xml) do
    # parse_string element count
    {:ok, rusty_count} = RustyXML.parse_string(xml, RustyCountHandler, 0)
    {:ok, saxy_count} = Saxy.parse_string(xml, SaxyCountHandler, 0)
    count_match = rusty_count == saxy_count
    IO.puts("Element count: RustyXML=#{rusty_count}, Saxy=#{saxy_count} - #{if count_match, do: "✓", else: "✗"}")

    # parse_string event collection — compare all collected events
    {:ok, rusty_events} = RustyXML.parse_string(xml, RustyCollectHandler, [])
    {:ok, saxy_events} = Saxy.parse_string(xml, SaxyCollectHandler, [])
    events_match = rusty_events == saxy_events
    IO.puts("Collected events: #{length(rusty_events)} vs #{length(saxy_events)} - #{if events_match, do: "✓", else: "✗"}")

    # SimpleForm — direct comparison (whitespace handling is identical)
    {:ok, rusty_simple} = RustyXML.SimpleForm.parse_string(xml)
    {:ok, saxy_simple} = Saxy.SimpleForm.parse_string(xml)
    simple_match = rusty_simple == saxy_simple
    IO.puts("SimpleForm: #{if simple_match, do: "✓", else: "✗"}")

    all_pass = count_match and events_match and simple_match
    IO.puts("\nOverall: #{if all_pass, do: "ALL TESTS PASSED ✓", else: "SOME TESTS FAILED ✗"}")

    %{count_match: count_match, events_match: events_match, simple_match: simple_match, all_pass: all_pass}
  end

  # =========================================================================
  # Results Output
  # =========================================================================

  defp save_results(timestamp, results, streaming, correctness, memory_tracking_enabled) do
    summary = build_summary(timestamp, results, streaming, correctness, memory_tracking_enabled)
    File.write!("#{@output_dir}/#{timestamp}_saxy_summary.md", summary)
  end

  defp build_summary(timestamp, results, streaming, correctness, memory_tracking_enabled) do
    """
    # Saxy Benchmark Results - #{timestamp}

    ## System
    - Elixir: #{System.version()}
    - OTP: #{System.otp_release()}
    - Schedulers: #{System.schedulers_online()}
    - NIF memory tracking: #{if memory_tracking_enabled, do: "ENABLED", else: "DISABLED"}

    ## Results

    | Operation | Scenario | XML Size | RustyXML ips | Saxy ips | Speedup |
    |-----------|----------|----------|--------------|----------|---------|
    #{format_results_table(results)}
    | parse_stream | Large (10K items) | #{format_size(streaming.xml_size)} | #{format_ips_from_time(streaming.rusty_time_us)} | #{format_ips_from_time(streaming.saxy_time_us)} | **#{streaming.speedup}x** |

    ## Memory Details

    | Operation | Scenario | NIF Peak | BEAM (RustyXML) | **Total (RustyXML)** | BEAM (Saxy) | Ratio |
    |-----------|----------|----------|-----------------|----------------------|-------------|-------|
    #{format_memory_table(results, memory_tracking_enabled)}
    | parse_stream | Large (10K items) | #{format_size(streaming.nif_peak)} | #{format_size(streaming.rusty_beam_mem)} | **#{format_size(streaming.nif_peak + streaming.rusty_beam_mem)}** | #{format_size(streaming.saxy_beam_mem)} | #{format_ratio(streaming.nif_peak + streaming.rusty_beam_mem, streaming.saxy_beam_mem)} |

    *parse_string memory: Benchee `memory_time` (total allocations per invocation)*
    *parse_stream memory: `:erlang.memory(:total)` delta (same method as sweet_bench)*

    ## Streaming Details

    | Metric | RustyXML | Saxy |
    |--------|----------|------|
    | Elements Counted | #{streaming.rusty_count} | #{streaming.saxy_count} |
    | Speedup | #{streaming.speedup}x | baseline |

    ## Correctness

    | Test | Status |
    |------|--------|
    | Element count | #{if correctness.count_match, do: "✓", else: "✗"} |
    | Collected events | #{if correctness.events_match, do: "✓", else: "✗"} |
    | SimpleForm | #{if correctness.simple_match, do: "✓", else: "✗"} |

    **Overall: #{if correctness.all_pass, do: "✓ ALL PASSED", else: "✗ SOME FAILED"}**

    ---
    Generated by `mix run bench/saxy_bench.exs`
    """
  end

  defp format_results_table(results) do
    results
    |> Enum.map(fn r ->
      "| #{r.operation} | #{r.scenario} | #{format_size(r.xml_size)} | #{format_ips(r.rusty_ips)} | #{format_ips(r.saxy_ips)} | **#{r.speedup}x** |"
    end)
    |> Enum.join("\n")
  end

  defp format_memory_table(results, memory_tracking_enabled) do
    results
    |> Enum.map(fn r ->
      total = r.nif_peak + r.rusty_beam_mem
      ratio = format_ratio(total, r.saxy_beam_mem)

      if memory_tracking_enabled do
        "| #{r.operation} | #{r.scenario} | #{format_size(r.nif_peak)} | #{format_size(r.rusty_beam_mem)} | **#{format_size(total)}** | #{format_size(r.saxy_beam_mem)} | #{ratio} |"
      else
        "| #{r.operation} | #{r.scenario} | N/A | #{format_size(r.rusty_beam_mem)} | **#{format_size(r.rusty_beam_mem)}** | #{format_size(r.saxy_beam_mem)} | #{ratio} |"
      end
    end)
    |> Enum.join("\n")
  end

  defp format_ratio(rusty, saxy) when saxy > 0 do
    "#{Float.round(rusty / saxy, 2)}x"
  end
  defp format_ratio(_, _), do: "N/A"

  defp format_ips_from_time(time_us) when time_us > 0 do
    format_ips(1_000_000 / time_us)
  end
  defp format_ips_from_time(_), do: "N/A"

  # =========================================================================
  # Formatters
  # =========================================================================

  defp format_size(bytes) when bytes >= 1_000_000, do: "#{Float.round(bytes / 1_000_000, 2)} MB"
  defp format_size(bytes) when bytes >= 1_000, do: "#{Float.round(bytes / 1_000, 1)} KB"
  defp format_size(bytes), do: "#{bytes} B"

  defp format_number(n) when n >= 1_000_000, do: "#{Float.round(n / 1_000_000, 2)}M"
  defp format_number(n) when n >= 1_000, do: "#{Float.round(n / 1_000, 1)}K"
  defp format_number(n), do: "#{n}"

  defp format_time(us) when us >= 1_000_000, do: "#{Float.round(us / 1_000_000, 2)}s"
  defp format_time(us) when us >= 1_000, do: "#{Float.round(us / 1_000, 2)}ms"
  defp format_time(us), do: "#{us}µs"

  defp format_time_us(us) when us >= 1_000_000, do: "#{Float.round(us / 1_000_000, 2)}s"
  defp format_time_us(us) when us >= 1_000, do: "#{Float.round(us / 1_000, 2)}ms"
  defp format_time_us(us), do: "#{Float.round(us, 1)}µs"

  defp format_ips(nil), do: "N/A"
  defp format_ips(ips) when ips >= 1_000_000, do: "#{Float.round(ips / 1_000_000, 2)}M"
  defp format_ips(ips) when ips >= 1_000, do: "#{Float.round(ips / 1_000, 2)}K"
  defp format_ips(ips), do: "#{Float.round(ips, 2)}"
end

SaxyBench.run()
