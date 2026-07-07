class Semtools < Formula
  desc "Semantic search and document parsing tools for the command-line"
  homepage "https://github.com/run-llama/semtools"
  version "3.0.1"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/run-llama/semtools/releases/download/v#{version}/semtools-aarch64-apple-darwin.tar.gz"
      sha256 "e43f20a5dc65e94b883158e7aca6dd11591840adc00db885a50024c38473f72b"
    else
      url "https://github.com/run-llama/semtools/releases/download/v#{version}/semtools-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000" # updated on release
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/run-llama/semtools/releases/download/v#{version}/semtools-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "b7a4dccd10f3ad7b544e65943216304fa59195f486d06e25ace6e2ade38a4d2c"
    else
      url "https://github.com/run-llama/semtools/releases/download/v#{version}/semtools-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f13d60ccbf64a9a7a9d8860d60f2b4fdf86a445d737220c5286b92e7eb87f54d"
    end
  end

  def install
    bin.install "semtools"
  end

  test do
    assert_match "Usage:", shell_output("#{bin}/semtools --help")
  end
end
