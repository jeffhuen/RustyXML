# Debug failing tests
xmlconf_base = "test/xmlconf/xmlconf"

test_suites = [
  {"xmltest/xmltest.xml", "xmltest/"},
  {"sun/sun-valid.xml", "sun/"},
  {"sun/sun-not-wf.xml", "sun/"},
  {"oasis/oasis.xml", "oasis/"},
  {"ibm/ibm_oasis_valid.xml", "ibm/"},
  {"ibm/ibm_oasis_not-wf.xml", "ibm/"}
]

extract_attr = fn attrs, name ->
  case Regex.run(~r/#{name}="([^"]*)"/, attrs) do
    [_, value] -> value
    _ -> nil
  end
end

safe_preview = fn xml ->
  # Check if it's UTF-16
  case xml do
    <<0xFF, 0xFE, _::binary>> -> "[UTF-16 LE encoded]"
    <<0xFE, 0xFF, _::binary>> -> "[UTF-16 BE encoded]"
    _ ->
      xml
      |> String.slice(0, 200)
      |> String.replace(~r/[^\x20-\x7E\n\r\t]/, "?")
      |> String.replace("\n", "\\n")
  end
end

IO.puts("=== VALID TESTS THAT FAIL (should accept but we reject) ===\n")

valid_failures = []

valid_failures = Enum.reduce(test_suites, valid_failures, fn {catalog_file, base_path}, acc ->
  catalog_path = Path.join(xmlconf_base, catalog_file)

  if File.exists?(catalog_path) do
    content = File.read!(catalog_path)

    Regex.scan(~r/<TEST\s+([^>]+)>([^<]*)<\/TEST>/s, content)
    |> Enum.reduce(acc, fn [_full, attrs, desc], inner_acc ->
      type = extract_attr.(attrs, "TYPE")
      entities = extract_attr.(attrs, "ENTITIES")
      uri = extract_attr.(attrs, "URI")
      id = extract_attr.(attrs, "ID")
      sections = extract_attr.(attrs, "SECTIONS")

      if entities == "none" and type == "valid" and uri do
        full_path = Path.join([xmlconf_base, base_path, uri])

        if File.exists?(full_path) do
          xml = File.read!(full_path)

          result = try do
            doc = RustyXML.parse(xml)
            RustyXML.root(doc)
            :ok
          rescue
            e -> {:error, Exception.message(e)}
          catch
            _, e -> {:error, inspect(e)}
          end

          case result do
            {:error, reason} ->
              [{id, sections, String.trim(desc), reason, full_path, safe_preview.(xml)} | inner_acc]
            _ -> inner_acc
          end
        else
          inner_acc
        end
      else
        inner_acc
      end
    end)
  else
    acc
  end
end)

Enum.each(valid_failures, fn {id, sections, desc, reason, path, preview} ->
  IO.puts("FAIL: #{id}")
  IO.puts("  Sections: #{sections}")
  IO.puts("  Description: #{desc}")
  IO.puts("  Error: #{reason}")
  IO.puts("  File: #{path}")
  IO.puts("  XML preview: #{preview}")
  IO.puts("")
end)

IO.puts("Total valid failures: #{length(valid_failures)}")

IO.puts("\n=== NOT-WF TESTS THAT PASS (should reject but we accept) ===\n")

# Categorize not-wf failures by section
notwf_failures = []

notwf_failures = Enum.reduce(test_suites, notwf_failures, fn {catalog_file, base_path}, acc ->
  catalog_path = Path.join(xmlconf_base, catalog_file)

  if File.exists?(catalog_path) do
    content = File.read!(catalog_path)

    Regex.scan(~r/<TEST\s+([^>]+)>([^<]*)<\/TEST>/s, content)
    |> Enum.reduce(acc, fn [_full, attrs, desc], inner_acc ->
      type = extract_attr.(attrs, "TYPE")
      entities = extract_attr.(attrs, "ENTITIES")
      uri = extract_attr.(attrs, "URI")
      id = extract_attr.(attrs, "ID")
      sections = extract_attr.(attrs, "SECTIONS")

      if entities == "none" and type == "not-wf" and uri do
        full_path = Path.join([xmlconf_base, base_path, uri])

        if File.exists?(full_path) do
          xml = File.read!(full_path)

          result = try do
            doc = RustyXML.parse(xml)
            RustyXML.root(doc)
            :ok
          rescue
            _ -> :error
          catch
            _, _ -> :error
          end

          if result == :ok do
            [{id, sections, String.trim(desc), full_path, safe_preview.(xml)} | inner_acc]
          else
            inner_acc
          end
        else
          inner_acc
        end
      else
        inner_acc
      end
    end)
  else
    acc
  end
end)

# Group by section
by_section = Enum.group_by(notwf_failures, fn {_id, sections, _desc, _path, _xml} -> sections end)

IO.puts("Not-WF failures by XML spec section:")
by_section
|> Enum.sort_by(fn {section, items} -> -length(items) end)
|> Enum.each(fn {section, items} ->
  IO.puts("  Section #{section}: #{length(items)} failures")
end)

IO.puts("\nTotal not-wf failures: #{length(notwf_failures)}")

# Show first 10 examples
IO.puts("\n=== First 10 not-wf examples ===\n")
notwf_failures
|> Enum.take(10)
|> Enum.each(fn {id, sections, desc, path, xml} ->
  IO.puts("#{id} (Section #{sections})")
  IO.puts("  #{desc}")
  IO.puts("  XML: #{xml}")
  IO.puts("")
end)
