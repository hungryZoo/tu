# typed: false
# frozen_string_literal: true

# Homebrew formula for `tu`.
#
# This file lives at Formula/tu.rb in the project repo itself, so users
# can tap and install with:
#
#   brew tap hungryZoo/tu https://github.com/hungryZoo/tu
#   brew install tu
#
# Bottles are not built; the formula points straight at the
# pre-compiled binaries that are attached to every GitHub release.
class Tu < Formula
  desc "Tiny TUI menu on top of tmux"
  homepage "https://github.com/hungryZoo/tu"
  version "1.0.0"
  license "MIT"

  depends_on "tmux"

  on_macos do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.0.0/tu-1.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "5a19585df56cd386c19a60a2e20ab0df897558e5778fb8f0903bf595d939c8c9"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.0.0/tu-1.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "cb78ed1e44de16b1649e6570416e3ae7286a95843423e71c83eeb65519852918"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/hungryZoo/tu/releases/download/v1.0.0/tu-1.0.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "9c64bfd76530a406b3089c253bfc45464f770db7f7967a3841ccb01ecfb50e0f"
    end
    on_intel do
      url "https://github.com/hungryZoo/tu/releases/download/v1.0.0/tu-1.0.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "0ecff45353939e69220e2b94690c07e9fc1e046dfc9864af63636bdf31c752ff"
    end
  end

  def install
    bin.install "tu"
    doc.install "README.md", "LICENSE"
  end

  test do
    assert_match "tu #{version}", shell_output("#{bin}/tu --version")
  end
end
