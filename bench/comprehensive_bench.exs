# Comprehensive XML Benchmark: RustyXML vs SweetXml
#
# Usage:
#   FORCE_RUSTYXML_BUILD=1 mix run bench/comprehensive_bench.exs
#
# For memory tracking (requires feature flag):
#   1. Edit native/rustyxml/Cargo.toml: default = ["mimalloc", "memory_tracking"]
#   2. FORCE_RUSTYXML_BUILD=1 mix compile --force
#   3. mix run bench/comprehensive_bench.exs
#
# Operations benchmarked:
#   - Parsing (RustyXML vs SweetXml)
#   - XPath queries (simple, complex, on raw XML vs pre-parsed)
#   - Streaming (stream_tags/3)
#   - Memory usage (Rust NIF tracking)
#   - BEAM reductions

defmodule ComprehensiveBench do
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
    IO.puts("COMPREHENSIVE XML BENCHMARK")
    IO.puts("RustyXML vs SweetXml")
    IO.puts("Timestamp: #{timestamp}")
    IO.puts(String.duplicate("=", 70))

    # System info
    print_system_info()

    # Check memory tracking
    check_memory_tracking()

    # Generate test documents
    test_docs = generate_test_docs()

    # 1. Parsing benchmarks (various sizes)
    run_parse_benchmark("Small Document", test_docs.small, test_docs.small_path)
    run_parse_benchmark("Medium Document", test_docs.medium, test_docs.medium_path)
    run_parse_benchmark("Large Document", test_docs.large, test_docs.large_path)

    # 2. XPath benchmarks
    run_xpath_simple_benchmark("Simple XPath", test_docs.medium)
    run_xpath_preparsed_benchmark("XPath on Pre-Parsed", test_docs.medium)
    run_xpath_complex_benchmark("Complex XPath", test_docs.large)

    # 3. Streaming benchmark
    run_streaming_benchmark(test_docs.large_path, test_docs.large_item_count)

    # 4. Memory comparison
    run_memory_comparison(test_docs.medium)

    # 5. Correctness verification
    verify_correctness(test_docs.medium)

    # Save results
    save_results(timestamp)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("BENCHMARK COMPLETE")
    IO.puts("Results saved to: #{@output_dir}/")
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
      IO.puts("Status: ENABLED (memory_tracking feature active)")
    else
      IO.puts("Status: DISABLED (returns 0 - enable memory_tracking feature for detailed stats)")
    end
  end

  defp generate_test_docs do
    IO.puts("\n--- Generating Test Documents ---")

    File.mkdir_p!("bench/data")

    # Small: 50 items
    small_path = "bench/data/small.xml"
    small = ensure_xml_file(small_path, 50)
    IO.puts("Small XML: #{format_size(byte_size(small))} (50 items)")

    # Medium: 1000 items
    medium_path = "bench/data/medium.xml"
    medium = ensure_xml_file(medium_path, 1_000)
    IO.puts("Medium XML: #{format_size(byte_size(medium))} (1,000 items)")

    # Large: 10000 items
    large_path = "bench/data/large.xml"
    large = ensure_xml_file(large_path, 10_000)
    IO.puts("Large XML: #{format_size(byte_size(large))} (10,000 items)")

    # Very large: 50000 items (for streaming)
    very_large_path = "bench/data/very_large.xml"
    _very_large = ensure_xml_file(very_large_path, 50_000)
    very_large_size = File.stat!(very_large_path).size
    IO.puts("Very Large XML: #{format_size(very_large_size)} (50,000 items)")

    %{
      small: small,
      small_path: small_path,
      medium: medium,
      medium_path: medium_path,
      large: large,
      large_path: large_path,
      large_item_count: 10_000,
      very_large_path: very_large_path,
      very_large_item_count: 50_000
    }
  end

  # ==========================================================================
  # Parsing Benchmarks
  # ==========================================================================

  defp run_parse_benchmark(name, xml, _path) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name} - Parse")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    # Warm up
    _ = RustyXML.parse(xml)
    _ = SweetXml.parse(xml)

    Benchee.run(
      %{
        "RustyXML.parse" => fn -> RustyXML.parse(xml) end,
        "SweetXml.parse" => fn -> SweetXml.parse(xml) end
      },
      warmup: 1,
      time: 3,
      memory_time: 1,
      print: [configuration: false]
    )
  end

  # ==========================================================================
  # XPath Benchmarks
  # ==========================================================================

  defp run_xpath_simple_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Operation: Parse + XPath (single query on raw XML)")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    # Warm up
    _ = RustyXML.xpath(xml, @rusty_items_l)
    _ = SweetXml.xpath(xml, @sweet_items_l)

    Benchee.run(
      %{
        "RustyXML (raw XML)" => fn -> RustyXML.xpath(xml, @rusty_items_l) end,
        "SweetXml (raw XML)" => fn -> SweetXml.xpath(xml, @sweet_items_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  defp run_xpath_preparsed_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Operation: XPath on pre-parsed document")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    # Pre-parse
    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    # Warm up
    _ = RustyXML.xpath(rusty_doc, @rusty_items_l)
    _ = SweetXml.xpath(sweet_doc, @sweet_items_l)

    IO.puts("\n1. Simple query (//item) - element list:")
    Benchee.run(
      %{
        "RustyXML raw (XML strings)" => fn -> RustyXML.Native.xpath_query_raw(rusty_doc, "//item") end,
        "RustyXML (nested tuples)" => fn -> RustyXML.xpath(rusty_doc, @rusty_items_l) end,
        "SweetXml (xmerl records)" => fn -> SweetXml.xpath(sweet_doc, @sweet_items_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n2. Text extraction (//item/name/text()):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_names_sl) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_names_sl) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n3. Attribute extraction (//item/@id):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_ids_sl) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_ids_sl) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  defp run_xpath_complex_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Operation: Complex XPath queries on large document")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    IO.puts("\n1. Predicate query ([@category='cat5']):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_pred_l) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_pred_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n2. Count function (count(//item)):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_count) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_count) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  # ==========================================================================
  # Streaming Benchmark
  # ==========================================================================

  defp run_streaming_benchmark(path, expected_count) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: Streaming (stream_tags/3)")
    IO.puts("File: #{path}")
    IO.puts("Expected items: #{format_number(expected_count)}")
    IO.puts(String.duplicate("-", 50))

    # RustyXML streaming
    IO.puts("\n1. RustyXML stream_tags/3:")
    {rusty_time, rusty_count} = :timer.tc(fn ->
      path
      |> RustyXML.stream_tags(:item)
      |> Enum.count()
    end)
    IO.puts("   Items: #{format_number(rusty_count)}")
    IO.puts("   Time: #{format_time(rusty_time)}")
    IO.puts("   Correct: #{if rusty_count == expected_count, do: "✓", else: "✗"}")

    # SweetXml streaming (requires File.stream! instead of path)
    IO.puts("\n2. SweetXml stream_tags/3:")
    {sweet_time, sweet_count} = :timer.tc(fn ->
      path
      |> File.stream!()
      |> SweetXml.stream_tags(:item)
      |> Enum.count()
    end)
    IO.puts("   Items: #{format_number(sweet_count)}")
    IO.puts("   Time: #{format_time(sweet_time)}")
    IO.puts("   Correct: #{if sweet_count == expected_count, do: "✓", else: "✗"}")

    # Comparison
    IO.puts("\n--- Comparison ---")
    if sweet_time > 0 do
      speedup = sweet_time / rusty_time
      IO.puts("RustyXML vs SweetXml: #{Float.round(speedup, 2)}x #{if speedup > 1, do: "faster", else: "slower"}")
    end

    # Test Stream.take (RustyXML should work, SweetXml may hang)
    IO.puts("\n3. Stream.take(5) test:")
    IO.puts("   RustyXML:")
    {rusty_take_time, rusty_take_result} = :timer.tc(fn ->
      path
      |> RustyXML.stream_tags(:item)
      |> Stream.take(5)
      |> Enum.to_list()
    end)
    IO.puts("      Items: #{length(rusty_take_result)}")
    IO.puts("      Time: #{format_time(rusty_take_time)}")
    IO.puts("      Status: ✓ (no hang)")

    IO.puts("\n   SweetXml:")
    IO.puts("      Note: SweetXml may hang on Stream.take (issue #97)")
    IO.puts("      Skipping to avoid benchmark hang")
  end

  # ==========================================================================
  # Memory Comparison
  # ==========================================================================

  defp run_memory_comparison(xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Memory Comparison")
    IO.puts("XML Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    IO.puts("\n=== Memory Measurement Methodology ===")
    IO.puts("- 'BEAM Heap': Memory delta in the calling process")
    IO.puts("- 'Rust NIF Peak': Peak allocation during parsing (requires memory_tracking)")
    IO.puts("- 'Rust NIF Retained': Memory held after parsing")
    IO.puts("- SweetXml allocates entirely on BEAM; RustyXML allocates on Rust side")

    # Rust memory (if tracking enabled)
    RustyXML.Native.reset_rust_memory_stats()
    _ = RustyXML.parse(xml)
    peak = RustyXML.Native.get_rust_memory_peak()

    if peak > 0 do
      IO.puts("\n1. Rust NIF Memory:")

      RustyXML.Native.reset_rust_memory_stats()
      _doc = RustyXML.parse(xml)
      parse_peak = RustyXML.Native.get_rust_memory_peak()
      parse_current = RustyXML.Native.get_rust_memory()

      IO.puts("   RustyXML.parse:")
      IO.puts("      Peak: #{format_size(parse_peak)}")
      IO.puts("      Retained: #{format_size(parse_current)}")
      IO.puts("      Peak/XML ratio: #{Float.round(parse_peak / byte_size(xml), 1)}x")
      IO.puts("      Retained/XML ratio: #{Float.round(parse_current / byte_size(xml), 1)}x")
    else
      IO.puts("\n1. Rust NIF Memory: DISABLED")
      IO.puts("   Enable memory_tracking feature for detailed stats")
    end

    # BEAM reductions
    IO.puts("\n2. BEAM Reductions (scheduler work):")

    rusty_reds = measure_reductions(fn -> RustyXML.parse(xml) end)
    sweet_reds = measure_reductions(fn -> SweetXml.parse(xml) end)

    IO.puts("   RustyXML.parse: #{format_number(rusty_reds)}")
    IO.puts("   SweetXml.parse: #{format_number(sweet_reds)}")

    if rusty_reds > 0 do
      ratio = sweet_reds / rusty_reds
      IO.puts("   Ratio: SweetXml uses #{Float.round(ratio, 1)}x more reductions")
    end

    IO.puts("\n   Note: Low reductions = less scheduler overhead")
    IO.puts("   Trade-off: NIFs can't be preempted mid-execution")
  end

  # ==========================================================================
  # Correctness Verification
  # ==========================================================================

  defp verify_correctness(xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Correctness Verification")
    IO.puts(String.duplicate("-", 50))

    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    # Test count
    rusty_count = RustyXML.xpath(rusty_doc, @rusty_count)
    sweet_count = SweetXml.xpath(sweet_doc, @sweet_count)
    count_match = rusty_count == sweet_count
    IO.puts("count(//item): RustyXML=#{rusty_count}, SweetXml=#{sweet_count} - #{if count_match, do: "✓", else: "✗"}")

    # Test string extraction
    rusty_name = RustyXML.xpath(rusty_doc, @rusty_first_name_s)
    sweet_name = SweetXml.xpath(sweet_doc, @sweet_first_name_s)
    name_match = rusty_name == sweet_name
    IO.puts("//item[1]/name/text(): RustyXML=\"#{rusty_name}\", SweetXml=\"#{sweet_name}\" - #{if name_match, do: "✓", else: "✗"}")

    # Test list extraction
    rusty_ids = RustyXML.xpath(rusty_doc, @rusty_ids_sl) |> length()
    sweet_ids = SweetXml.xpath(sweet_doc, @sweet_ids_sl) |> length()
    ids_match = rusty_ids == sweet_ids
    IO.puts("//item/@id count: RustyXML=#{rusty_ids}, SweetXml=#{sweet_ids} - #{if ids_match, do: "✓", else: "✗"}")

    all_pass = count_match and name_match and ids_match
    IO.puts("\nOverall: #{if all_pass, do: "ALL TESTS PASSED ✓", else: "SOME TESTS FAILED ✗"}")
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
          <description>This is a detailed description for product #{i} with some text content.</description>
          <price currency="USD">#{:rand.uniform(10000) / 100}</price>
          <quantity>#{:rand.uniform(100)}</quantity>
          <tags>
            <tag>tag#{rem(i, 5)}</tag>
            <tag>tag#{rem(i, 7)}</tag>
          </tags>
        </item>
        """
      end)
      |> Enum.join("\n")

    """
    <?xml version="1.0" encoding="UTF-8"?>
    <catalog xmlns="http://example.com/catalog" version="1.0">
      <metadata>
        <created>2024-01-15</created>
        <author>Benchmark Generator</author>
        <item_count>#{count}</item_count>
      </metadata>
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
  # Helpers
  # ==========================================================================

  defp save_results(timestamp) do
    summary = """
    # XML Benchmark Results - #{timestamp}

    ## System
    - Elixir: #{System.version()}
    - OTP: #{System.otp_release()}
    - Schedulers: #{System.schedulers_online()}
    - RustyXML: #{Application.spec(:rusty_xml, :vsn)}
    - SweetXml: #{Application.spec(:sweet_xml, :vsn)}

    ## Operations Tested
    - Parsing (small, medium, large documents)
    - XPath queries (simple, complex, pre-parsed)
    - Streaming (stream_tags/3)
    - Memory usage
    - BEAM reductions

    ## Notes
    Results printed to console. For detailed analysis, re-run with:
    ```
    FORCE_RUSTYXML_BUILD=1 mix run bench/comprehensive_bench.exs 2>&1 | tee bench/results/#{timestamp}.txt
    ```
    """

    File.write!("#{@output_dir}/#{timestamp}_summary.md", summary)
  end

  defp measure_reductions(fun) do
    {:reductions, before} = Process.info(self(), :reductions)
    _ = fun.()
    {:reductions, after_reds} = Process.info(self(), :reductions)
    after_reds - before
  end

  defp format_size(bytes) when bytes >= 1_000_000, do: "#{Float.round(bytes / 1_000_000, 2)} MB"
  defp format_size(bytes) when bytes >= 1_000, do: "#{Float.round(bytes / 1_000, 1)} KB"
  defp format_size(bytes), do: "#{bytes} B"

  defp format_number(n) when n >= 1_000_000, do: "#{Float.round(n / 1_000_000, 2)}M"
  defp format_number(n) when n >= 1_000, do: "#{Float.round(n / 1_000, 1)}K"
  defp format_number(n), do: "#{n}"

  defp format_time(microseconds) when microseconds >= 1_000_000 do
    "#{Float.round(microseconds / 1_000_000, 2)}s"
  end
  defp format_time(microseconds) when microseconds >= 1_000 do
    "#{Float.round(microseconds / 1_000, 2)}ms"
  end
  defp format_time(microseconds), do: "#{microseconds}µs"
end

ComprehensiveBench.run()
