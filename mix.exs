defmodule RustyXML.MixProject do
  use Mix.Project

  @version "0.1.1"
  @source_url "https://github.com/jeffhuen/rustyxml"

  def project do
    [
      app: :rusty_xml,
      version: @version,
      elixir: "~> 1.14",
      start_permanent: Mix.env() == :prod,
      deps: deps(),

      # Hex
      description: description(),
      package: package(),

      # Docs
      name: "RustyXML",
      docs: docs()
    ]
  end

  def application do
    [
      extra_applications: [:logger]
    ]
  end

  defp description do
    """
    Ultra-fast XML parser and XPath 1.0 engine for Elixir, built from scratch as a
    Rust NIF. 100% W3C/OASIS XML Conformance (1089/1089 tests). 6-40x faster than
    xmerl/SweetXml. Drop-in SweetXml replacement with ~x sigil and streaming support.
    """
  end

  defp package do
    [
      name: "rusty_xml",
      maintainers: ["Jeff Huen"],
      licenses: ["MIT"],
      links: %{
        "GitHub" => @source_url,
        "Changelog" => "#{@source_url}/blob/main/CHANGELOG.md"
      },
      files: ~w(
        lib
        native/rustyxml/src
        native/rustyxml/Cargo.toml
        native/rustyxml/Cargo.lock
        checksum-Elixir.RustyXML.Native.exs
        .formatter.exs
        mix.exs
        README.md
        LICENSE
        CHANGELOG.md
        docs
      )
    ]
  end

  defp docs do
    [
      main: "readme",
      name: "RustyXML",
      source_ref: "v#{@version}",
      source_url: @source_url,
      homepage_url: @source_url,
      extras: [
        "README.md": [title: "Overview"],
        "CHANGELOG.md": [title: "Changelog"],
        "docs/ARCHITECTURE.md": [title: "Architecture"],
        "docs/BENCHMARK.md": [title: "Benchmarks"],
        "docs/COMPLIANCE.md": [title: "XML Compliance"],
        LICENSE: [title: "License"]
      ],
      groups_for_extras: [
        Guides: ~r/docs\/.*/
      ],
      groups_for_modules: [
        Core: [
          RustyXML
        ],
        Streaming: [
          RustyXML.Streaming
        ],
        "Low-Level": [
          RustyXML.Native
        ]
      ]
    ]
  end

  defp deps do
    [
      {:rustler, "~> 0.37", optional: true},
      {:rustler_precompiled, "~> 0.8"},
      {:sweet_xml, "~> 0.7", only: [:dev, :test]},
      {:benchee, "~> 1.0", only: :dev},
      {:credo, "~> 1.7", only: [:dev, :test], runtime: false},
      {:dialyxir, "~> 1.4", only: [:dev, :test], runtime: false},
      {:ex_doc, "~> 0.31", only: :dev, runtime: false}
    ]
  end
end
