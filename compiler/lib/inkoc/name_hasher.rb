# frozen_string_literal: true

module Inkoc
  class NameHasher
    def initialize
      @hashes = {}
      @counter = -1
    end

    # Generates a globally unique hash for a symbol name.
    def generate(name)
      # TODO: we can just increment an integer, that's basically the same as
      # hashing for globally unique IDs.
      #
      # TODO: this will produce bucket collisions quite often.
      #
      # TODO: this allows for up to 2**64 unique names. Should be enough.
      #
      # TODO: given a class C, we can find all methods that share a bucket. Per
      # bucket, we then order them by the number of calls (most first). This way
      # more frequently called methods need fewer probes.
      #
      # TODO: we can then slap a poly inline cache on top to further reduce
      # overhead.

      if (existing = @hashes[name])
        # Previously calculated hashes must be reused. If we don't, then hashing
        # a value that conflicts twice could produce different hashes.
        existing
      else
        # Since unique names must resolve to unique IDs, we can just increment
        # an integer; instead of using an actual hashing algorithm (e.g. FNV).
        @hashes[name] = @counter += 1
      end
    end
  end
end
