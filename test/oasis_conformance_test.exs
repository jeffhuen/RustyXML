defmodule OasisConformanceTest do
  @moduledoc """
  OASIS/W3C XML Conformance Test Suite Runner

  Tests RustyXML against the official W3C XML Conformance Test Suite (xmlconf).
  This suite contains 2000+ test cases from Sun, IBM, OASIS/NIST, and others.

  ## Setup

  The test data is not included in the repository (it's ~50MB). To run these tests:

      # Download the W3C XML Conformance Test Suite
      mkdir -p test/xmlconf
      cd test/xmlconf
      curl -LO https://www.w3.org/XML/Test/xmlts20130923.tar.gz
      tar -xzf xmlts20130923.tar.gz
      rm xmlts20130923.tar.gz

  Then run:

      mix test test/oasis_conformance_test.exs

  Or run only specific categories:

      mix test test/oasis_conformance_test.exs --only valid
      mix test test/oasis_conformance_test.exs --only not_wf

  ## Test Categories

  - `valid`: Well-formed AND valid XML - parser must accept
  - `not-wf`: Not well-formed XML - parser must reject
  - `invalid`: Well-formed but invalid (DTD) - skipped (non-validating parser)
  - `error`: Optional errors - skipped

  ## Entity Handling

  We only run tests with ENTITIES="none" since we don't expand external entities.
  This is a security feature (prevents XXE attacks), not a limitation.

  ## References

  - W3C Test Suite: https://www.w3.org/XML/Test/
  - OASIS Committee: https://www.oasis-open.org/committees/xml-conformance/
  """

  use ExUnit.Case, async: true

  @xmlconf_base "test/xmlconf/xmlconf"

  # Parse the test catalog files and extract test cases
  @test_suites [
    {"xmltest/xmltest.xml", "xmltest/"},
    {"sun/sun-valid.xml", "sun/"},
    {"sun/sun-not-wf.xml", "sun/"},
    {"oasis/oasis.xml", "oasis/"},
    {"ibm/ibm_oasis_valid.xml", "ibm/"},
    {"ibm/ibm_oasis_not-wf.xml", "ibm/"}
  ]

  # Collect all test cases at compile time
  @test_cases (
    extract_attr_fn = fn attrs, name ->
      case Regex.run(~r/#{name}="([^"]*)"/, attrs) do
        [_, value] -> value
        _ -> nil
      end
    end

    for {catalog_file, base_path} <- @test_suites,
        catalog_path = Path.join(@xmlconf_base, catalog_file),
        File.exists?(catalog_path) do
      content = File.read!(catalog_path)

      # Parse TEST elements - simple regex extraction
      Regex.scan(
        ~r/<TEST\s+([^>]+)>([^<]*)<\/TEST>/s,
        content
      )
      |> Enum.map(fn [_full, attrs, description] ->
        type = extract_attr_fn.(attrs, "TYPE")
        entities = extract_attr_fn.(attrs, "ENTITIES")
        id = extract_attr_fn.(attrs, "ID")
        uri = extract_attr_fn.(attrs, "URI")
        sections = extract_attr_fn.(attrs, "SECTIONS")

        full_path =
          if uri, do: Path.join([@xmlconf_base, base_path, uri]), else: nil

        %{
          id: id,
          type: type,
          entities: entities,
          uri: uri,
          sections: sections,
          description: String.trim(description),
          base_path: base_path,
          full_path: full_path
        }
      end)
      |> Enum.filter(fn test ->
        # Only run tests with no entity requirements and valid paths
        test.entities == "none" and
          test.type in ["valid", "not-wf"] and
          test.uri != nil and
          test.id != nil
      end)
    end
    |> List.flatten()
  )

  # Generate test cases
  describe "Valid (must accept)" do
    @valid_tests Enum.filter(@test_cases, &(&1.type == "valid"))

    for test <- @valid_tests do
      @tag :oasis
      @tag :valid
      @tag sections: test.sections
      test "#{test.id}" do
        test_case = unquote(Macro.escape(test))
        run_valid_test(test_case)
      end
    end
  end

  # Not-WF tests are expected to fail (RustyXML is a lenient parser)
  # Run with: mix test test/oasis_conformance_test.exs --only not_wf
  describe "Not-WF (must reject) - EXPECTED FAILURES" do
    @not_wf_tests Enum.filter(@test_cases, &(&1.type == "not-wf"))

    for test <- @not_wf_tests do
      @tag :oasis
      @tag :not_wf
      @tag sections: test.sections
      test "#{test.id}" do
        test_case = unquote(Macro.escape(test))
        run_not_wf_test(test_case)
      end
    end
  end

  # Test runners
  defp run_valid_test(test) do
    path = test.full_path

    if File.exists?(path) do
      xml = File.read!(path)

      case safe_parse(xml) do
        {:ok, _doc} ->
          :ok

        {:error, reason} ->
          flunk("""
          Parser rejected valid XML

          Test ID: #{test.id}
          File: #{path}
          Sections: #{test.sections}
          Description: #{test.description}
          Error: #{inspect(reason)}
          """)
      end
    else
      # Skip missing files
      :ok
    end
  end

  defp run_not_wf_test(test) do
    path = test.full_path

    if File.exists?(path) do
      xml = File.read!(path)

      case safe_parse(xml) do
        {:ok, _doc} ->
          flunk("""
          Parser accepted not-well-formed XML

          Test ID: #{test.id}
          File: #{path}
          Sections: #{test.sections}
          Description: #{test.description}
          """)

        {:error, _reason} ->
          :ok
      end
    else
      # Skip missing files
      :ok
    end
  end

  # Safely attempt to parse, catching any errors
  defp safe_parse(xml) do
    try do
      doc = RustyXML.parse(xml)
      # Verify we can get the root - this forces full parsing
      _root = RustyXML.root(doc)
      {:ok, doc}
    rescue
      e in [RustyXML.ParseError, ArgumentError, ErlangError] ->
        {:error, e}
    catch
      :error, reason ->
        {:error, reason}
    end
  end
end
