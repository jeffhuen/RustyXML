# RustyXML vs SweetXml Benchmark
#
# Usage:
#   mix run bench/xml_bench.exs
#
# For memory tracking (requires feature flag):
#   1. Edit native/rustyxml/Cargo.toml: default = ["mimalloc", "memory_tracking"]
#   2. FORCE_RUSTYXML_BUILD=true mix compile --force
#   3. mix run bench/xml_bench.exs

defmodule XMLBench do
  @output_dir "bench/results"

  # Pre-compile all XPath specs at compile time to avoid sigil conflicts
  # RustyXML specs
  require RustyXML
  @rusty_items_l %RustyXML.SweetXpath{path: "//item", is_list: true}
  @rusty_names_sl %RustyXML.SweetXpath{path: "//item/name/text()", is_list: true, cast_to: :string}
  @rusty_pred_l %RustyXML.SweetXpath{path: "//item[@category='cat5']", is_list: true}
  @rusty_tags_sl %RustyXML.SweetXpath{path: "//items//tag/text()", is_list: true, cast_to: :string}
  @rusty_count %RustyXML.SweetXpath{path: "count(//item)"}
  @rusty_multi_l %RustyXML.SweetXpath{path: "//item[@category='cat3'][position() < 100]", is_list: true}
  @rusty_name_s %RustyXML.SweetXpath{path: "//item[1]/name/text()", cast_to: :string}
  @rusty_qty_i %RustyXML.SweetXpath{path: "//item[1]/quantity/text()", cast_to: :integer}
  @rusty_cats_sl %RustyXML.SweetXpath{path: "//item/@category", is_list: true, cast_to: :string}
  @rusty_ids_sl %RustyXML.SweetXpath{path: "//item/@id", is_list: true, cast_to: :string}

  # SweetXml specs - use their struct format (SweetXpath is a separate module)
  require SweetXml
  @sweet_items_l %SweetXpath{path: ~c"//item", is_list: true}
  @sweet_names_sl %SweetXpath{path: ~c"//item/name/text()", is_list: true, cast_to: :string}
  @sweet_pred_l %SweetXpath{path: ~c"//item[@category='cat5']", is_list: true}
  @sweet_tags_sl %SweetXpath{path: ~c"//items//tag/text()", is_list: true, cast_to: :string}
  @sweet_count %SweetXpath{path: ~c"count(//item)"}
  @sweet_multi_l %SweetXpath{path: ~c"//item[@category='cat3'][position() < 100]", is_list: true}
  @sweet_name_s %SweetXpath{path: ~c"//item[1]/name/text()", cast_to: :string}
  @sweet_qty_i %SweetXpath{path: ~c"//item[1]/quantity/text()", cast_to: :integer}
  @sweet_cats_sl %SweetXpath{path: ~c"//item/@category", is_list: true, cast_to: :string}
  @sweet_ids_sl %SweetXpath{path: ~c"//item/@id", is_list: true, cast_to: :string}

  def run do
    File.mkdir_p!(@output_dir)
    timestamp = DateTime.utc_now() |> DateTime.to_iso8601(:basic) |> String.slice(0..14)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("RUSTYXML vs SWEETXML BENCHMARK")
    IO.puts("Timestamp: #{timestamp}")
    IO.puts(String.duplicate("=", 70))

    print_system_info()

    # Generate test XML documents
    test_docs = generate_test_docs()

    # Run benchmarks
    run_parse_benchmark("Small Document", test_docs.small)
    run_parse_benchmark("Medium Document", test_docs.medium)
    run_parse_benchmark("Large Document", test_docs.large)

    run_xpath_benchmark("Simple XPath", test_docs.medium)
    run_xpath_benchmark_complex("Complex XPath", test_docs.large)

    run_xmap_benchmark("xmap Extraction", test_docs.medium)

    run_sigil_benchmark("Sigil Modifiers", test_docs.medium)

    # Correctness verification
    verify_correctness(test_docs.medium)

    IO.puts("\n" <> String.duplicate("=", 70))
    IO.puts("BENCHMARK COMPLETE")
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

  defp generate_test_docs do
    IO.puts("\n--- Generating Test Documents ---")

    # Small: 50 items
    small = generate_xml(50)
    IO.puts("Small XML: #{format_size(byte_size(small))} (50 items)")

    # Medium: 1000 items
    medium = generate_xml(1000)
    IO.puts("Medium XML: #{format_size(byte_size(medium))} (1000 items)")

    # Large: 10000 items
    large = generate_xml(10_000)
    IO.puts("Large XML: #{format_size(byte_size(large))} (10000 items)")

    %{small: small, medium: medium, large: large}
  end

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

  # ==========================================================================
  # Parse Benchmarks
  # ==========================================================================

  defp run_parse_benchmark(name, xml) do
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

  defp run_xpath_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    # Pre-parse for fair comparison
    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    IO.puts("\n1. Parse + XPath (single query):")
    Benchee.run(
      %{
        "RustyXML (raw)" => fn -> RustyXML.xpath(xml, @rusty_items_l) end,
        "SweetXml (raw)" => fn -> SweetXml.xpath(xml, @sweet_items_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n2. XPath on pre-parsed document:")
    Benchee.run(
      %{
        "RustyXML (doc)" => fn -> RustyXML.xpath(rusty_doc, @rusty_items_l) end,
        "SweetXml (doc)" => fn -> SweetXml.xpath(sweet_doc, @sweet_items_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n3. Text extraction with sigil:")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_names_sl) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_names_sl) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  defp run_xpath_benchmark_complex(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    IO.puts("\n1. Predicate query:")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_pred_l) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_pred_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n2. Descendant axis:")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_tags_sl) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_tags_sl) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n3. Count function:")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_count) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_count) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )

    IO.puts("\n4. Multiple predicates:")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_multi_l) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_multi_l) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  # ==========================================================================
  # xmap Benchmark
  # ==========================================================================

  defp run_xmap_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts("Size: #{format_size(byte_size(xml))}")
    IO.puts(String.duplicate("-", 50))

    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    rusty_spec = [
      item_count: @rusty_count,
      first_name: @rusty_name_s,
      categories: @rusty_cats_sl
    ]

    sweet_spec = [
      item_count: @sweet_count,
      first_name: @sweet_name_s,
      categories: @sweet_cats_sl
    ]

    IO.puts("\n1. xmap with multiple queries:")
    Benchee.run(
      %{
        "RustyXML.xmap" => fn -> RustyXML.xmap(rusty_doc, rusty_spec) end,
        "SweetXml.xmap" => fn -> SweetXml.xmap(sweet_doc, sweet_spec) end
      },
      warmup: 1,
      time: 3,
      print: [configuration: false]
    )
  end

  # ==========================================================================
  # Sigil Modifier Benchmarks
  # ==========================================================================

  defp run_sigil_benchmark(name, xml) do
    IO.puts("\n" <> String.duplicate("-", 50))
    IO.puts("Benchmark: #{name}")
    IO.puts(String.duplicate("-", 50))

    rusty_doc = RustyXML.parse(xml)
    sweet_doc = SweetXml.parse(xml)

    IO.puts("\n1. String extraction (s modifier):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_name_s) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_name_s) end
      },
      warmup: 1,
      time: 2,
      print: [configuration: false]
    )

    IO.puts("\n2. Integer extraction (i modifier):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_qty_i) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_qty_i) end
      },
      warmup: 1,
      time: 2,
      print: [configuration: false]
    )

    IO.puts("\n3. List of strings (sl modifier):")
    Benchee.run(
      %{
        "RustyXML" => fn -> RustyXML.xpath(rusty_doc, @rusty_names_sl) end,
        "SweetXml" => fn -> SweetXml.xpath(sweet_doc, @sweet_names_sl) end
      },
      warmup: 1,
      time: 2,
      print: [configuration: false]
    )
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
    rusty_name = RustyXML.xpath(rusty_doc, @rusty_name_s)
    sweet_name = SweetXml.xpath(sweet_doc, @sweet_name_s)
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
  # Helpers
  # ==========================================================================

  defp format_size(bytes) when bytes >= 1_000_000, do: "#{Float.round(bytes / 1_000_000, 2)} MB"
  defp format_size(bytes) when bytes >= 1_000, do: "#{Float.round(bytes / 1_000, 1)} KB"
  defp format_size(bytes), do: "#{bytes} B"
end

XMLBench.run()
