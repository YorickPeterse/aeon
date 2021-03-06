# frozen_string_literal: true

module Inkoc
  module AST
    class Global
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :name, :location, :receiver

      # name - The name of the constant as a String.
      # receiver - The object to search for the constant.
      def initialize(name, location)
        @name = name
        @location = location
      end

      def visitor_method
        :on_global
      end

      def global?
        true
      end
    end
  end
end
