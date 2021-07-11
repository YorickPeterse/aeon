# frozen_string_literal: true

module Inkoc
  module AST
    class ForDestructure
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :variables, :location

      def initialize(variables, location)
        @variables = variables
        @location = location
      end
    end
  end
end
