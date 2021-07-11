# frozen_string_literal: true

module Inkoc
  class Symbol
    include Inspect

    attr_reader :name, :index, :references
    attr_accessor :type

    def initialize(name, type, index = -1, mutable = false)
      @name = name
      @type = type
      @index = index
      @mutable = mutable
      @references = 0
    end

    def any?
      true
    end

    def used?
      @references.positive?
    end

    def increment_references
      @references += 1
    end

    def mutable?
      @mutable
    end

    def or_else
      self
    end

    def type_or_else
      type
    end

    def ==(other)
      other.is_a?(Symbol) &&
        name == other.name &&
        type == other.type &&
        index == other.index &&
        mutable? == other.mutable?
    end

    def with_temporary_type(type)
      old_type = @type
      @type = type

      yield
    ensure
      @type = old_type
    end
  end

  class NullSymbol
    include Inspect

    attr_reader :name, :type, :index

    def self.singleton
      NULL_SYMBOL_SINGLETON
    end

    def initialize(name)
      @name = name
      @type = TypeSystem::Any.new
      @index = -1
    end

    def any?
      false
    end

    def mutable?
      false
    end

    def nil?
      true
    end

    def or_else
      yield
    end

    def type_or_else
      yield
    end
  end

  NULL_SYMBOL_SINGLETON = NullSymbol.new('<unknown>')
end
