defmodule RustyXML.MixProject do
  use Mix.Project

  @version "0.1.0"
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
    Ultra-fast XML parsing for Elixir. A purpose-built Rust NIF with SIMD acceleration,
    arena-based DOM, and full XPath 1.0 support. Drop-in replacement for SweetXml.
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
        LICENSE: [title: "License"]
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
