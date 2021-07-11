# frozen_string_literal: true

module Inkoc
  module AST
    class ForBinding
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :name, :mutable, :value_type, :location

      def initialize(name, mutable, vtype, location)
        @name = name
        @mutable = mutable
        @value_type = vtype
        @location = location
      end
    end
  end
end
