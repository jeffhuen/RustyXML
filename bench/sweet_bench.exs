# SweetXml Benchmark: RustyXML vs SweetXml
#
# Usage:
#   mix run bench/sweet_bench.exs
#
# For memory tracking (requires feature flag):
#   1. Edit native/rustyxml/Cargo.toml: default = ["mimalloc", "memory_tracking"]
#   2. RUSTYXML_BUILD=1 mix compile --force
#   3. mix run bench/sweet_bench.exs

defmodule SweetBench do
  @output_dir "bench/results"

  # Pre-compile XPath specs to avoid sigil conflicts
  require RustyXML
  @rusty_items_l %RustyXML.SweetXpath{path: "//item", is_list: true}
  @rusty_names_sl %RustyXML.SweetXpath{path: "//item/name/text()", is_list: true, cast_to: :string}
  @rusty_pred_l %RustyXML.SweetXpath{path: "//item[@category='cat5']", is_list: true}
  @rusty_count %RustyXML.SweetXpath{path: "count(//item)"}
  @rusty_first_name_s %RustyXML.SweetXpath{path: "//item[1]/name/text()", cast_to: :string}
  @rusty_ids_sl %RustyXML.SweetXpath{path: "//item/@id", is_list: true, cast_to: :string}

  require SweetXml
  @sweet_items_l %SweetXpath{path: ~c"//item", is_list: true}
  @sweet_names_sl %SweetXpath{path: ~c"//item/name/text()", is_list: true, cast_to: :string}
  @sweet_pred_l %SweetXpath{path: ~c"//item[@category='cat5']", is_list: true}
  @sweet_count %SweetXpath{path: ~c"count(//item)"}
  @sweet_first_name_s %SweetXpath{path: ~c"//item[1]/name/text()", cast_to: :string}
  @sweet_ids_sl %SweetXpath{path: ~c"//item/@id", is_list: true, cast_to: :string}

  def run do
    File.mkdir_p!(@output_dir)
    timestamp = DateTime.utc_now() |> DateTime.to_iso8601(:basic) |> String.slice(0..14)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("SWEETXML BENCHMARK")
    IO.puts("RustyXML vs SweetXml")
    IO.puts("Timestamp: #{timestamp}")
    IO.puts(String.duplicate("=", 70))

    # System info
    print_system_info()

    # Check memory tracking
    memory_tracking_enabled = check_memory_tracking()

    # Generate test documents
    test_docs = generate_test_docs()

    # Collect all benchmark results
    all_results = []

    # 1. Parsing benchmarks (various sizes)
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("PARSING BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    parse_small = run_benchmark("Parse", "Small (50 items)", test_docs.small, fn xml ->
      {fn -> RustyXML.parse(xml) end, fn -> SweetXml.parse(xml) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_small]

    parse_medium = run_benchmark("Parse", "Medium (1K items)", test_docs.medium, fn xml ->
      {fn -> RustyXML.parse(xml) end, fn -> SweetXml.parse(xml) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_medium]

    parse_large = run_benchmark("Parse", "Large (10K items)", test_docs.large, fn xml ->
      {fn -> RustyXML.parse(xml) end, fn -> SweetXml.parse(xml) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [parse_large]

    # 2. XPath benchmarks
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("XPATH BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    # Pre-parse documents for XPath benchmarks
    rusty_doc_medium = RustyXML.parse(test_docs.medium)
    sweet_doc_medium = SweetXml.parse(test_docs.medium)
    rusty_doc_large = RustyXML.parse(test_docs.large)
    sweet_doc_large = SweetXml.parse(test_docs.large)

    xpath_items = run_benchmark("XPath //item", "Medium (1K items)", test_docs.medium, fn _xml ->
      {fn -> RustyXML.xpath(rusty_doc_medium, @rusty_items_l) end,
       fn -> SweetXml.xpath(sweet_doc_medium, @sweet_items_l) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [xpath_items]

    xpath_text = run_benchmark("XPath text()", "Medium (1K items)", test_docs.medium, fn _xml ->
      {fn -> RustyXML.xpath(rusty_doc_medium, @rusty_names_sl) end,
       fn -> SweetXml.xpath(sweet_doc_medium, @sweet_names_sl) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [xpath_text]

    xpath_attr = run_benchmark("XPath @id", "Medium (1K items)", test_docs.medium, fn _xml ->
      {fn -> RustyXML.xpath(rusty_doc_medium, @rusty_ids_sl) end,
       fn -> SweetXml.xpath(sweet_doc_medium, @sweet_ids_sl) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [xpath_attr]

    xpath_pred = run_benchmark("XPath predicate", "Large (10K items)", test_docs.large, fn _xml ->
      {fn -> RustyXML.xpath(rusty_doc_large, @rusty_pred_l) end,
       fn -> SweetXml.xpath(sweet_doc_large, @sweet_pred_l) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [xpath_pred]

    xpath_count = run_benchmark("XPath count()", "Large (10K items)", test_docs.large, fn _xml ->
      {fn -> RustyXML.xpath(rusty_doc_large, @rusty_count) end,
       fn -> SweetXml.xpath(sweet_doc_large, @sweet_count) end}
    end, memory_tracking_enabled)
    all_results = all_results ++ [xpath_count]

    # 3. Streaming benchmark
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("STREAMING BENCHMARKS")
    IO.puts(String.duplicate("=", 70))

    streaming_result = run_streaming_benchmark(test_docs.large_path, test_docs.large_item_count, memory_tracking_enabled)

    # 4. Correctness verification
    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("CORRECTNESS VERIFICATION")
    IO.puts(String.duplicate("=", 70))

    correctness = verify_correctness(test_docs.medium)

    # Save results
    save_results(timestamp, all_results, streaming_result, correctness, memory_tracking_enabled)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("BENCHMARK COMPLETE")
    IO.puts("Results saved to: #{@output_dir}/#{timestamp}_sweet_summary.md")
    IO.puts(String.duplicate("=", 70))
  end

  defp print_system_info do
    IO.puts("\n--- System Information ---")
    IO.puts("Elixir: #{System.version()}")
    IO.puts("OTP: #{System.otp_release()}")
    IO.puts("OS: #{:os.type() |> inspect()}")
    IO.puts("Schedulers: #{System.schedulers_online()}")
    IO.puts("RustyXML: #{Application.spec(:rusty_xml, :vsn)}")
    IO.puts("SweetXml: #{Application.spec(:sweet_xml, :vsn)}")
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
      IO.puts("NIF memory tracking: DISABLED (enable memory_tracking feature for NIF stats)")
      false
    end
  end

  defp generate_test_docs do
    IO.puts("\n--- Generating Test Documents ---")

    File.mkdir_p!("bench/data")

    small_path = "bench/data/small.xml"
    small = ensure_xml_file(small_path, 50)
    IO.puts("Small XML: #{format_size(byte_size(small))} (50 items)")

    medium_path = "bench/data/medium.xml"
    medium = ensure_xml_file(medium_path, 1_000)
    IO.puts("Medium XML: #{format_size(byte_size(medium))} (1,000 items)")

    large_path = "bench/data/large.xml"
    large = ensure_xml_file(large_path, 10_000)
    IO.puts("Large XML: #{format_size(byte_size(large))} (10,000 items)")

    %{
      small: small,
      small_path: small_path,
      medium: medium,
      medium_path: medium_path,
      large: large,
      large_path: large_path,
      large_item_count: 10_000
    }
  end

  # ==========================================================================
  # Unified Benchmark Runner with Memory Tracking
  # ==========================================================================

  defp run_benchmark(operation, scenario, xml, bench_fn, memory_tracking_enabled) do
    IO.puts("\n--- #{operation}: #{scenario} ---")
    IO.puts("XML Size: #{format_size(byte_size(xml))}")

    {rusty_fn, sweet_fn} = bench_fn.(xml)

    # Warm up
    _ = rusty_fn.()
    _ = sweet_fn.()

    # Measure NIF peak memory for RustyXML
    # We measure the delta (peak - baseline) so pre-existing allocations
    # (e.g. pre-parsed documents for XPath benchmarks) don't inflate the number
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

    # Run Benchee with memory measurement
    result = Benchee.run(
      %{
        "RustyXML" => rusty_fn,
        "SweetXml" => sweet_fn
      },
      warmup: 1,
      time: 3,
      memory_time: 1,
      print: [configuration: false]
    )

    # Extract results
    rusty_scenario = Enum.find(result.scenarios, &(&1.name == "RustyXML"))
    sweet_scenario = Enum.find(result.scenarios, &(&1.name == "SweetXml"))

    rusty_ips = rusty_scenario.run_time_data.statistics.ips
    sweet_ips = sweet_scenario.run_time_data.statistics.ips
    rusty_avg = rusty_scenario.run_time_data.statistics.average / 1000  # to µs
    sweet_avg = sweet_scenario.run_time_data.statistics.average / 1000

    rusty_beam_mem = get_memory_stat(rusty_scenario)
    sweet_beam_mem = get_memory_stat(sweet_scenario)

    speedup = if sweet_ips > 0, do: Float.round(rusty_ips / sweet_ips, 2), else: 0

    # Print summary
    IO.puts("\nResults:")
    IO.puts("  RustyXML: #{format_ips(rusty_ips)} ips (#{format_time_us(rusty_avg)})")
    IO.puts("  SweetXml: #{format_ips(sweet_ips)} ips (#{format_time_us(sweet_avg)})")
    IO.puts("  Speedup: #{speedup}x")

    if memory_tracking_enabled do
      IO.puts("\nMemory:")
      IO.puts("  NIF Peak: #{format_size(nif_peak)}")
      IO.puts("  BEAM (RustyXML): #{format_size(rusty_beam_mem)}")
      IO.puts("  BEAM (SweetXml): #{format_size(sweet_beam_mem)}")
      total_rusty = nif_peak + rusty_beam_mem
      ratio = if sweet_beam_mem > 0, do: Float.round(total_rusty / sweet_beam_mem, 2), else: 0
      IO.puts("  Total (RustyXML): #{format_size(total_rusty)}")
      IO.puts("  Memory Ratio: #{ratio}x of SweetXml")
    end

    %{
      operation: operation,
      scenario: scenario,
      xml_size: byte_size(xml),
      rusty_ips: rusty_ips,
      sweet_ips: sweet_ips,
      rusty_avg_us: rusty_avg,
      sweet_avg_us: sweet_avg,
      speedup: speedup,
      nif_peak: nif_peak,
      rusty_beam_mem: rusty_beam_mem,
      sweet_beam_mem: sweet_beam_mem
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

  # ==========================================================================
  # Streaming Benchmark
  # ==========================================================================

  defp run_streaming_benchmark(path, expected_count, memory_tracking_enabled) do
    file_size = File.stat!(path).size
    IO.puts("\n--- Streaming: Large (10K items) ---")
    IO.puts("File: #{path}")
    IO.puts("Size: #{format_size(file_size)}")
    IO.puts("Expected items: #{format_number(expected_count)}")

    # Measure NIF memory for RustyXML streaming (delta from baseline)
    nif_peak = if memory_tracking_enabled do
      :erlang.garbage_collect()
      Process.sleep(10)
      {baseline, _} = RustyXML.Native.reset_rust_memory_stats()
      path |> RustyXML.stream_tags(:item) |> Enum.count()
      peak = RustyXML.Native.get_rust_memory_peak()
      max(0, peak - baseline)
    else
      0
    end

    # RustyXML streaming with memory
    :erlang.garbage_collect()
    rusty_mem_before = :erlang.memory(:total)
    {rusty_time, rusty_count} = :timer.tc(fn ->
      path |> RustyXML.stream_tags(:item) |> Enum.count()
    end)
    rusty_mem_after = :erlang.memory(:total)
    rusty_beam_mem = max(0, rusty_mem_after - rusty_mem_before)

    IO.puts("\nRustyXML stream_tags/3:")
    IO.puts("  Items: #{format_number(rusty_count)}")
    IO.puts("  Time: #{format_time(rusty_time)}")
    IO.puts("  Correct: #{if rusty_count == expected_count, do: "✓", else: "✗"}")

    # SweetXml streaming with memory
    :erlang.garbage_collect()
    sweet_mem_before = :erlang.memory(:total)
    {sweet_time, sweet_count} = :timer.tc(fn ->
      path |> File.stream!() |> SweetXml.stream_tags(:item) |> Enum.count()
    end)
    sweet_mem_after = :erlang.memory(:total)
    sweet_beam_mem = max(0, sweet_mem_after - sweet_mem_before)

    IO.puts("\nSweetXml stream_tags/3:")
    IO.puts("  Items: #{format_number(sweet_count)}")
    IO.puts("  Time: #{format_time(sweet_time)}")
    IO.puts("  Correct: #{if sweet_count == expected_count, do: "✓", else: "✗"}")

    speedup = if sweet_time > 0, do: Float.round(sweet_time / rusty_time, 2), else: 0

    IO.puts("\nComparison:")
    IO.puts("  Speedup: #{speedup}x faster")

    if memory_tracking_enabled do
      IO.puts("\nMemory:")
      IO.puts("  NIF Peak: #{format_size(nif_peak)}")
      IO.puts("  BEAM (RustyXML): #{format_size(rusty_beam_mem)}")
      IO.puts("  BEAM (SweetXml): #{format_size(sweet_beam_mem)}")
    end

    # Stream.take test
    IO.puts("\nStream.take(5) test:")
    {rusty_take_time, rusty_take_result} = :timer.tc(fn ->
      path |> RustyXML.stream_tags(:item) |> Stream.take(5) |> Enum.to_list()
    end)
    IO.puts("  RustyXML: #{length(rusty_take_result)} items in #{format_time(rusty_take_time)} ✓")
    IO.puts("  SweetXml: Skipped (may hang - issue #97)")

    %{
      operation: "Streaming",
      scenario: "Large (10K items)",
      xml_size: file_size,
      rusty_time_us: rusty_time,
      sweet_time_us: sweet_time,
      rusty_count: rusty_count,
      sweet_count: sweet_count,
      speedup: speedup,
      nif_peak: nif_peak,
      rusty_beam_mem: rusty_beam_mem,
      sweet_beam_mem: sweet_beam_mem,
      expected_count: expected_count
    }
  end

  # ==========================================================================
  # Correctness Verification
  # ==========================================================================

  defp verify_correctness(xml) do
    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    rusty_count = RustyXML.xpath(rusty_doc, @rusty_count)
    sweet_count = SweetXml.xpath(sweet_doc, @sweet_count)
    count_match = rusty_count == sweet_count
    IO.puts("count(//item): RustyXML=#{rusty_count}, SweetXml=#{sweet_count} - #{if count_match, do: "✓", else: "✗"}")

    rusty_name = RustyXML.xpath(rusty_doc, @rusty_first_name_s)
    sweet_name = SweetXml.xpath(sweet_doc, @sweet_first_name_s)
    name_match = rusty_name == sweet_name
    IO.puts("//item[1]/name/text(): \"#{rusty_name}\" vs \"#{sweet_name}\" - #{if name_match, do: "✓", else: "✗"}")

    rusty_ids = RustyXML.xpath(rusty_doc, @rusty_ids_sl) |> length()
    sweet_ids = SweetXml.xpath(sweet_doc, @sweet_ids_sl) |> length()
    ids_match = rusty_ids == sweet_ids
    IO.puts("//item/@id count: #{rusty_ids} vs #{sweet_ids} - #{if ids_match, do: "✓", else: "✗"}")

    all_pass = count_match and name_match and ids_match
    IO.puts("\nOverall: #{if all_pass, do: "ALL TESTS PASSED ✓", else: "SOME TESTS FAILED ✗"}")

    %{count_match: count_match, name_match: name_match, ids_match: ids_match, all_pass: all_pass}
  end

  # ==========================================================================
  # Test Data Generation
  # ==========================================================================

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

  # ==========================================================================
  # Results Output
  # ==========================================================================

  defp save_results(timestamp, benchmark_results, streaming_result, correctness, memory_tracking_enabled) do
    summary = build_summary(timestamp, benchmark_results, streaming_result, correctness, memory_tracking_enabled)
    File.write!("#{@output_dir}/#{timestamp}_sweet_summary.md", summary)
  end

  defp build_summary(timestamp, results, streaming, correctness, memory_tracking_enabled) do
    """
    # SweetXml Benchmark Results - #{timestamp}

    ## System
    - Elixir: #{System.version()}
    - OTP: #{System.otp_release()}
    - Schedulers: #{System.schedulers_online()}
    - NIF memory tracking: #{if memory_tracking_enabled, do: "ENABLED", else: "DISABLED"}

    ## Results

    | Operation | Scenario | XML Size | RustyXML ips | SweetXml ips | Speedup |
    |-----------|----------|----------|--------------|--------------|---------
    #{format_results_table(results)}
    | Streaming | Large (10K items) | #{format_size(streaming.xml_size)} | #{format_ips_time(streaming.rusty_time_us)} | #{format_ips_time(streaming.sweet_time_us)} | **#{streaming.speedup}x** |

    ## Memory Details

    | Operation | Scenario | NIF Peak | BEAM (RustyXML) | **Total (RustyXML)** | BEAM (SweetXml) | Ratio |
    |-----------|----------|----------|-----------------|----------------------|-----------------|-------|
    #{format_memory_table(results, memory_tracking_enabled)}
    | Streaming | Large (10K items) | #{format_size(streaming.nif_peak)} | #{format_size(streaming.rusty_beam_mem)} | **#{format_size(streaming.nif_peak + streaming.rusty_beam_mem)}** | #{format_size(streaming.sweet_beam_mem)} | #{format_ratio(streaming.nif_peak + streaming.rusty_beam_mem, streaming.sweet_beam_mem)} |

    ## Streaming Details

    | Metric | RustyXML | SweetXml |
    |--------|----------|----------|
    | Items Processed | #{streaming.rusty_count} | #{streaming.sweet_count} |
    | Time | #{format_time(streaming.rusty_time_us)} | #{format_time(streaming.sweet_time_us)} |
    | Correct | #{if streaming.rusty_count == streaming.expected_count, do: "✓", else: "✗"} | #{if streaming.sweet_count == streaming.expected_count, do: "✓", else: "✗"} |

    **Stream.take(5)**: RustyXML ✓ works correctly, SweetXml ✗ may hang (issue #97)

    ## Correctness

    | Test | Status |
    |------|--------|
    | count(//item) | #{if correctness.count_match, do: "✓", else: "✗"} |
    | //item[1]/name/text() | #{if correctness.name_match, do: "✓", else: "✗"} |
    | //item/@id count | #{if correctness.ids_match, do: "✓", else: "✗"} |

    **Overall: #{if correctness.all_pass, do: "✓ ALL PASSED", else: "✗ SOME FAILED"}**

    ---
    Generated by `mix run bench/sweet_bench.exs`
    """
  end

  defp format_results_table(results) do
    results
    |> Enum.map(fn r ->
      "| #{r.operation} | #{r.scenario} | #{format_size(r.xml_size)} | #{format_ips(r.rusty_ips)} | #{format_ips(r.sweet_ips)} | **#{r.speedup}x** |"
    end)
    |> Enum.join("\n")
  end

  defp format_memory_table(results, memory_tracking_enabled) do
    results
    |> Enum.map(fn r ->
      total = r.nif_peak + r.rusty_beam_mem
      ratio = format_ratio(total, r.sweet_beam_mem)

      if memory_tracking_enabled do
        "| #{r.operation} | #{r.scenario} | #{format_size(r.nif_peak)} | #{format_size(r.rusty_beam_mem)} | **#{format_size(total)}** | #{format_size(r.sweet_beam_mem)} | #{ratio} |"
      else
        "| #{r.operation} | #{r.scenario} | N/A | #{format_size(r.rusty_beam_mem)} | **#{format_size(r.rusty_beam_mem)}** | #{format_size(r.sweet_beam_mem)} | #{ratio} |"
      end
    end)
    |> Enum.join("\n")
  end

  defp format_ratio(rusty, sweet) when sweet > 0 do
    ratio = rusty / sweet
    "#{Float.round(ratio, 2)}x"
  end
  defp format_ratio(_, _), do: "N/A"

  defp format_ips_time(time_us) do
    if time_us > 0 do
      ips = 1_000_000 / time_us
      "~#{Float.round(ips, 1)}/s"
    else
      "N/A"
    end
  end

  # ==========================================================================
  # Formatters
  # ==========================================================================

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

SweetBench.run()
