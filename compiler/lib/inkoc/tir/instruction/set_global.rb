# frozen_string_literal: true

module Inkoc
  module TIR
    module Instruction
      class SetGlobal
        include Predicates
        include Inspect

        attr_reader :variable, :value, :location

        def initialize(variable, value, location)
          @variable = variable
          @value = value
          @location = location
        end

        def register
          nil
        end

        def visitor_method
          :on_set_global
        end
      end
    end
  end
end
